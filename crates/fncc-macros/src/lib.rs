use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, ReturnType, Type, TypeReference, parse_macro_input};

/// Marks a function as a fncc command handler.
///
/// # Level 1 — No arguments
/// ```ignore
/// #[fncc::command]
/// pub fn handle_click() {
///     println!("clicked!");
/// }
/// ```
///
/// # Level 2 — Event argument
/// ```ignore
/// #[fncc::command]
/// pub fn handle_click(event: &ClickEvent) {
///     println!("clicked at {:?}", event.position);
/// }
/// ```
///
/// # Level 3 — State + context
/// ```ignore
/// #[fncc::command]
/// pub fn handle_click(state: &mut CounterState, cx: &mut Context<CounterState>) {
///     state.count += 1;
///     cx.notify();
/// }
/// ```
#[proc_macro_attribute]
pub fn command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let name = &func.sig.ident;
    let trampoline_name = syn::Ident::new(&format!("__fncc_cmd_{name}"), name.span());

    let is_return_ok = match &func.sig.output {
        ReturnType::Default => true,
        ReturnType::Type(_, ty) => is_unit(ty.as_ref()),
    };

    let arg_count = func.sig.inputs.len();

    if !is_return_ok {
        let msg = format!("`#[fncc::command]` on `{name}`: command must return ()");
        return syn::Error::new_spanned(&func.sig.output, msg).to_compile_error().into();
    }

    let expanded = match arg_count {
        0 => {
            // Level 1: trampoline matching on_click signature, ignores everything
            quote! {
                #func

                #[allow(unused)]
                fn #trampoline_name(
                    _event: &ClickEvent,
                    _window: &mut Window,
                    _cx: &mut App,
                ) {
                    #name();
                }
            }
        }
        1 => {
            // Level 2: trampoline passes event reference
            // Extract first param type
            let first_param = func.sig.inputs.first().unwrap();
            let event_type = extract_param_type(first_param);

            quote! {
                #func

                #[allow(unused)]
                fn #trampoline_name(
                    event: &#event_type,
                    _window: &mut Window,
                    _cx: &mut App,
                ) {
                    #name(event);
                }
            }
        }
        2 => {
            // Level 3: trampoline takes (&mut State, &mut Context<State>)
            // Extract state type from first parameter (&mut CounterState → CounterState)
            let state_type = extract_state_type(&func.sig.inputs);
            let trampoline_ty = state_type.clone();

            quote! {
                #func

                #[allow(unused)]
                fn #trampoline_name(state: &mut #trampoline_ty, cx: &mut Context<#trampoline_ty>) {
                    #name(state, cx);
                }
            }
        }
        n => {
            let msg = format!("`#[fncc::command]` on `{name}`: expected 0, 1, or 2 arguments, got {n}");
            return syn::Error::new_spanned(&func.sig, msg).to_compile_error().into();
        }
    };

    expanded.into()
}

/// Extract the type from a function parameter (skipping `self`-like patterns).
fn extract_param_type(arg: &FnArg) -> Type {
    match arg {
        FnArg::Typed(pat_type) => (*pat_type.ty).clone(),
        FnArg::Receiver(_) => {
            // Fallback for self: use Self type
            syn::parse_quote!(Self)
        }
    }
}

/// Extract the state type from `&mut CounterState` → `CounterState`.
fn extract_state_type(inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>) -> Type {
    if let Some(FnArg::Typed(pat)) = inputs.first() {
        if let Type::Reference(TypeReference { elem, .. }) = pat.ty.as_ref() {
            // remove the inner &mut wrapper — yield the pointee type
            return elem.as_ref().clone();
        }
        // fallback: return the type as-is
        return (*pat.ty).clone();
    }
    syn::parse_quote!(Self)
}

fn is_unit(ty: &Type) -> bool {
    match ty {
        Type::Tuple(tup) => tup.elems.is_empty(),
        _ => false,
    }
}
