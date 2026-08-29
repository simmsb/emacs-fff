use std::{path::PathBuf, time::Duration};

use emacs::{defun, Env, IntoLisp, Result, Value, Vector};

use fff_search::{
    FilePicker, FilePickerOptions, FrecencyTracker, FuzzySearchOptions, GrepConfig,
    GrepSearchOptions, QueryParser, QueryTracker, SharedFilePicker, SharedFrecency,
    SharedQueryTracker,
};
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

emacs::plugin_is_GPL_compatible!();

#[emacs::module(name = "fff-module")]
fn init(_: &Env) -> Result<()> {
    Ok(())
}

struct EmacsFilePicker {
    file_picker: SharedFilePicker,
    // frecency: SharedFrecency,
    query_tracker: SharedQueryTracker,
    // picker: FilePicker,
}

#[defun(user_ptr)]
fn new_file_picker(database_path: String, base_path: String) -> Result<EmacsFilePicker> {
    let database_path = PathBuf::try_from(database_path)?;
    std::fs::create_dir_all(&database_path)?;

    let shared_file_picker = SharedFilePicker::default();
    let shared_frecency = SharedFrecency::default();
    let frecency = FrecencyTracker::open(database_path.join("frecency"))?;
    shared_frecency.init(frecency)?;
    let shared_query_tracker = SharedQueryTracker::default();
    let query_tracker = QueryTracker::open(database_path.join("query"))?;
    shared_query_tracker.init(query_tracker)?;

    FilePicker::new_with_shared_state(
        shared_file_picker.clone(),
        shared_frecency,
        FilePickerOptions {
            base_path: base_path.try_into()?,
            enable_mmap_cache: true,
            enable_content_indexing: true,
            mode: fff_search::FFFMode::Neovim,
            cache_budget: None,
            watch: true,
            follow_symlinks: true,
            enable_fs_root_scanning: false,
            enable_home_dir_scanning: false,
        },
    )?;

    Ok(EmacsFilePicker {
        file_picker: shared_file_picker,
        query_tracker: shared_query_tracker,
    })
}

#[defun]
fn poll_file_picker_scan(fff: &mut EmacsFilePicker, timeout: u64) -> Result<bool> {
    Ok(fff
        .file_picker
        .wait_for_scan(Duration::from_millis(timeout)))
}

#[defun]
fn poll_file_picker_indexed(fff: &mut EmacsFilePicker, timeout: u64) -> Result<bool> {
    Ok(fff
        .file_picker
        .wait_for_indexing_complete(Duration::from_millis(timeout)))
}

#[defun]
fn fuzzy_file_search<'e>(
    env: &'e Env,
    fff: &mut EmacsFilePicker,
    query: String,
) -> Result<Vector<'e>> {
    let picker_guard = fff.file_picker.read()?;
    let picker = picker_guard.as_ref().unwrap();
    let qt_guard = fff.query_tracker.read()?;

    let query = QueryParser::default().parse(&query);

    let results = picker.fuzzy_search(&query, qt_guard.as_ref(), FuzzySearchOptions::default());

    let r = make_vector(
        env,
        results
            .items
            .iter()
            .zip(results.match_byte_offsets)
            .map(|(file_item, offsets)| {
                let file_path = file_item.relative_path(picker);
                let offsets = make_vector(
                    env,
                    offsets
                        .into_iter()
                        .map(|(start, end)| env.cons(start, end).unwrap()),
                )
                .unwrap();
                env.cons(file_path, offsets).unwrap()
            }),
    )?;

    Ok(r)
}

fn string_byte_idxs_to_char_idx(s: &str, indexes: &mut [(u32, u32)]) {
    let mut indexes = indexes.into_iter().flat_map(|(x, y)| [x, y]);
    let mut char_indexes = s.char_indices().enumerate();

    let Some(mut idx) = indexes.next() else {
        return;
    };
    let Some((mut char_idx, (mut byte_idx, _))) = char_indexes.next() else {
        return;
    };

    loop {
        while (byte_idx as u32) < *idx {
            let Some((char_idx_, (byte_idx_, _))) = char_indexes.next() else {
                return;
            };
            char_idx = char_idx_;
            byte_idx = byte_idx_;
        }

        while *idx > byte_idx as u32 {
            let Some(idx_) = indexes.next() else { return };
            idx = idx_;
        }

        if *idx == (byte_idx as u32) {
            *idx = char_idx as u32;
            let Some(idx_) = indexes.next() else { return };
            idx = idx_;
        }
    }
}

#[defun]
fn fuzzy_grep_search<'e>(
    env: &'e Env,
    fff: &mut EmacsFilePicker,
    query: String,
) -> Result<Value<'e>> {
    let picker_guard = fff.file_picker.read()?;
    let picker = picker_guard.as_ref().unwrap();

    let query = QueryParser::new(GrepConfig).parse(&query);

    let mut results = picker.grep(
        &query,
        &GrepSearchOptions {
            page_limit: 10_000,
            mode: fff_search::GrepMode::Regex,
            ..GrepSearchOptions::default()
        },
    );

    for result in &mut results.matches {
        string_byte_idxs_to_char_idx(
            &result.line_content,
            result.match_byte_offsets.as_mut_slice(),
        );
    }

    let r = make_vector(
        env,
        results.matches.iter().map(|match_item| {
            let file_idx = match_item.file_index;
            let offsets = make_vector(
                env,
                match_item
                    .match_byte_offsets
                    .iter()
                    .map(|(start, end)| env.cons(*start, *end).unwrap()),
            )
            .unwrap();
            env.list([
                file_idx.into_lisp(env).unwrap(),
                match_item.line_number.into_lisp(env).unwrap(),
                (&match_item.line_content).into_lisp(env).unwrap(),
                offsets.into_lisp(env).unwrap(),
            ])
            .unwrap()
        }),
    )?;

    let file_names = make_vector(env, results.files.iter().map(|f| f.relative_path(picker)))?;

    env.cons(r, file_names)
}

fn make_vector<'e, T: IntoLisp<'e>, I: Iterator<Item = T> + ExactSizeIterator>(
    env: &'e Env,
    it: impl IntoIterator<IntoIter = I>,
) -> Result<Vector<'e>> {
    let it = it.into_iter();
    let vector = env.make_vector(it.len(), ())?;

    for (idx, x) in it.enumerate() {
        vector.set(idx, x)?;
    }

    Ok(vector)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn check_byte_idx() {
        // ü is two bytes, so everything after it should have the byte index
        // decremented by one.
        let s = "foo ü uuu";

        let indexes = &mut [(0, 3), (4, 6), (8, 9)];
        string_byte_idxs_to_char_idx(s, indexes);

        assert_eq!(indexes, &[(0, 3), (4, 5), (7, 8)]);
    }
}
