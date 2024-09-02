use wasmtime::{
    component::{bindgen, Component, Linker},
    Config, Engine, Store,
};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder};
bindgen!({
    world: "example",
    path: "../timer-wasm/wit",
    // ...
});
struct ComponentState {
    table: ResourceTable,
    wasi_ctx: WasiCtx,
}

impl wasmtime_wasi::WasiView for ComponentState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }
}
fn main() {
    let mut config = Config::default();
    config.wasm_component_model(true);
    let engine = Engine::new(&config).unwrap();
    let component = Component::from_file(
        &engine,
        "../timer-wasm/target/wasm32-wasip1/debug/timer_wasm.wasm",
    )
    .unwrap();
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_sync(&mut linker).unwrap();
    let table = ResourceTable::new();
    let mut wasi = WasiCtxBuilder::new();
    wasi.inherit_stdio();
    let wasi_ctx = wasi.build();
    let mut store = Store::new(&engine, ComponentState { table, wasi_ctx });
    let bindings = Example::instantiate(&mut store, &component, &linker).unwrap();
    bindings.call_run_timer_example(&mut store).unwrap()
}
