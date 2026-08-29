;;; -*- lexical-binding: t; -*-
;; Copyright (C) 2026 Ben Simms

;; Author: Ben Simms
;; Homepage: https://github.com/simmsb/emacs-fff
;; Package-Requires: ((emacs "29.1") (consult "3.3"))
;; Version: 0.0.1
;; Keywords: convenience

;;; Code:

(require 'fff-module)
(require 'consult)
(eval-when-compile (require 'cl-lib))

(defcustom fff-cache-dir
  (temporary-file-directory)
  "Location to store fff databases in"
  :type 'string)

(defvar fff--searchers (make-hash-table :test 'equal))

(defun fff--get-searcher (base-path)
  (let ((val (gethash base-path fff--searchers)))
    (unless val
      (let* ((path-hash (sha1 base-path))
             (database-path (file-name-concat fff-cache-dir "fff" path-hash))
             (searcher (fff-module-new-file-picker database-path base-path)))
        (puthash base-path searcher fff--searchers)
        searcher))
    val))

(defun fff--do-search (base-path input)
  (let* ((searcher (fff--get-searcher base-path)))
    (while (not (fff-module-poll-file-picker-indexed searcher 10)))
    (pcase (fff-module-fuzzy-grep-search searcher input)
      (`(,results . ,filepaths)
       (cl-map 'vector (pcase-lambda (`(,file-idx . ,rest))
                         (let* ((filepath (elt filepaths file-idx)))
                           (cons filepath rest)))
               results)))))

(defun fff--consult-highlight-matches (line match-regions)
  (let ((line-len (length line)))
    (seq-doseq (elt match-regions)
      (pcase-let ((`(,match-start . ,match-end) elt))
        (when (and (< match-start line-len) (<= match-end line-len))
          (add-face-text-property match-start match-end 'consult-highlight-match nil line))))))

(defun fff--consult-format-candidates (result query)
  (cl-map 'list (pcase-lambda (`(,filepath ,line-number ,line ,match-regions))
                  (fff--consult-highlight-matches line match-regions)
                  (let* ((file-len (length filepath))
                         (line-number-str (number-to-string line-number))
                         (line-number-len (length line-number-str)))
                    (when (and consult-grep-max-columns
                               (length> line consult-grep-max-columns))
                      (setq line (substring line 0 consult-grep-max-columns)))
                    (setq str (concat filepath ":" line-number-str ":" line))
                    (add-text-properties 0 file-len `(face consult-file consult--prefix-group ,filepath) str)
                    (put-text-property (1+ file-len) (+ 1 file-len line-number-len) 'face 'consult-line-number str)
                    str))
          result))

(defun fff--consult-grep (prompt dir initial)
  "Asynchronous FFF grep."
  (consult--read
   (consult--dynamic-collection
       (lambda (input callback)
         (funcall callback (fff--consult-format-candidates (fff--do-search dir input) input))))
   :prompt prompt
   :lookup #'consult--lookup-member
   :state (consult--grep-state)
   :initial initial
   :add-history (thing-at-point 'symbol)
   :require-match t
   :category 'consult-grep
   :group #'consult--prefix-group
   :history '(:input consult--fff-history)
   :async-wrap #'consult--async-wrap
   :sort nil))

;;;###autoload
(defun fff-consult-grep (&optional dir initial)
  "Search with `fff' for files in DIR where the content matches the search.

  The initial input is given by the INITIAL argument."
  (interactive "P")
  (pcase-let* ((`(,prompt ,paths ,dir) (consult--directory-prompt "Grep" dir))
               (default-directory dir))
     (fff--consult-grep prompt dir initial)))

(provide 'fff-search)
