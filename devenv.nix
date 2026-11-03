{ pkgs, lib, config, inputs, ... }:

{
  packages = with pkgs; [
    just

    # https://github.com/dmtrKovalenko/zlob/issues/38
    zig_0_16
    cargo-zigbuild
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    targets = [ "x86_64-apple-darwin" ];
  };
}
