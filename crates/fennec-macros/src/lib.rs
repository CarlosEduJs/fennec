use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ReturnType};

/// Marks a function as a Fennec command handler.
///
/// # Level 1 — No arguments
/// ```ignore
/// #[fennec::command]
/// pub fn handle_click() {
///     println!("clicked!");
/// }
/// ```
///
/// # Level 2 — Event argument
/// ```ignore
/// #[fennec::command]
/// pub fn handle_click(event: &ClickEvent) {
///     println!("clicked at {:?}", event.position);
/// }
/// ```
#[proc_macro_attribute]
pub fn command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let name = &func.sig.ident;
    let trampoline_name = syn::Ident::new(
        &format!("__fennec_cmd_{name}"),
        name.span(),
    );

    let is_return_ok = match &func.sig.output {
        ReturnType::Default => true,
        ReturnType::Type(_, ty) => is_unit(ty.as_ref()),
    };

    match func.sig.inputs.len() {
        0 => {
            if !is_return_ok {
                let msg = format!(
                    "`#[fennec::command]` on `{name}` (Level 1): command must return ()"
                );
                return syn::Error::new_spanned(&func.sig.output, msg)
                    .to_compile_error()
                    .into();
            }

            // Level 1: trampoline ignores all args
            let expanded = quote! {
                #func

                #[allow(unused)]
                fn #trampoline_name(
                    event: &ClickEvent,
                    _window: &mut Window,
                    _cx: &mut App,
                ) {
                    let _ = event;
                    #name();
                }
            };
            return expanded.into();
        }
        1 => {
            // Level 2: trampoline passes event by reference
            let expanded = quote! {
                #func

                #[allow(unused)]
                fn #trampoline_name(
                    event: &ClickEvent,
                    _window: &mut Window,
                    _cx: &mut App,
                ) {
                    #name(event);
                }
            };
            return expanded.into();
        }
        n => {
            let msg = format!(
                "`#[fennec::command]` on `{name}`: expected 0 or 1 argument, got {n}"
            );
            return syn::Error::new_spanned(&func.sig, msg)
                .to_compile_error()
                .into();
        }
    }
}

fn is_unit(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Tuple(tup) => tup.elems.is_empty(),
        _ => false,
    }
}
