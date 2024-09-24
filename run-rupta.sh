cargo clean
export LD_LIBRARY_PATH=$(rustc --print sysroot)/lib:$LD_LIBRARY_PATH
cargo-pta pta -- --entry-func main --dump-call-graph cg.dot --dump-pts pts.txt