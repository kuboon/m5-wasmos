//! usage: precompile <in.wasm> <out.cwasm>
//!        precompile --engine-id
fn main() -> wasmtime::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--engine-id") {
        println!("{}", wasmos_host::engine::ENGINE_ID);
        return Ok(());
    }
    let [_, input, output] = args.as_slice() else {
        eprintln!("usage: precompile <in.wasm> <out.cwasm>");
        std::process::exit(2);
    };
    let engine = wasmos_host::engine::engine()?;
    let cwasm = engine.precompile_component(&std::fs::read(input)?)?;
    std::fs::write(output, cwasm)?;
    Ok(())
}
