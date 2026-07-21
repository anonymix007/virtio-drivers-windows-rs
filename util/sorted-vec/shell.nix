with import <nixpkgs> {};
mkShell {
  buildInputs = [
    cargo-udeps
    glab
    rustup
  ];
}
