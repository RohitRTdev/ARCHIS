use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn init(
    _attr: TokenStream,
    item: TokenStream,
) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);

    let body = func.block;

    if func.sig.ident != "driver_init" {
        return syn::Error::new_spanned(
            &func.sig.ident,
            "Expected module init fn: driver_init",
        )
        .to_compile_error()
        .into(); 
    }

    quote! {
        static MODULE_NAME_STR: &'static str = env!("CARGO_PKG_NAME");

        #[cfg(not(test))]
        #[panic_handler]
        fn panic(info: &core::panic::PanicInfo) -> ! {
            let message = info.message().as_str().or(Some("Panicking!")).unwrap();
            let mod_name = common::StrRef::from_str(MODULE_NAME_STR);
            let message_ref = common::StrRef::from_str(message);
            unsafe {kernel_intf::panic_router(mod_name, message_ref)}
        }

        #[unsafe(no_mangle)]
        extern "C" fn module_config() -> common::StrRef {
            kernel_intf::init_logger(
                MODULE_NAME_STR
            );

            kernel_intf::enable_timestamp();

            common::StrRef::from_str(
                MODULE_NAME_STR
            )
        }

        mod import_stub;
        use import_stub::*;

        #[unsafe(no_mangle)]
        extern "C" fn driver_init() {
            #body
        }
    }
    .into()
}

#[proc_macro_attribute]
pub fn export(
    _attr: TokenStream,
    item: TokenStream,
) -> TokenStream {
    let mut func = parse_macro_input!(item as ItemFn);

    func.sig.abi = Some(syn::parse_quote!(
        extern "C"
    ));

    let attrs = &func.attrs;
    let vis = &func.vis;
    let sig = &func.sig;
    let block = &func.block;

    quote! {
        #[unsafe(no_mangle)]
        #(#attrs)*
        #vis
        #sig
        #block
    }
    .into()
}