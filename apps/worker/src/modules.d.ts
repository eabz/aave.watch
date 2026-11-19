// Bundled by the rules in wrangler.jsonc: fonts as bytes, the renderer as a compiled module.
declare module "*.ttf" {
	const bytes: ArrayBuffer;
	export default bytes;
}
declare module "*.wasm" {
	const module: WebAssembly.Module;
	export default module;
}
