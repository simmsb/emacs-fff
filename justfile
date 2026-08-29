macos:
  cargo zigbuild --release --target universal2-apple-darwin
  cp target/universal2-apple-darwin/release/libemacs_fff.dylib fff-module.dylib
