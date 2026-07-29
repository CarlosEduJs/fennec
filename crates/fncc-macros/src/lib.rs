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

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    // --- extract_param_type ---

    #[test]
    fn test_extract_param_type_from_typed_arg() {
        let arg: FnArg = parse_quote!(_: &ClickEvent);
        let ty = extract_param_type(&arg);
        assert!(
            matches!(&ty, Type::Reference(TypeReference { elem, .. }) if matches!(elem.as_ref(), Type::Path(p) if p.qself.is_none() && p.path.is_ident("ClickEvent")))
        );
    }

    #[test]
    fn test_extract_param_type_from_mut_ref() {
        let arg: FnArg = parse_quote!(_: &mut CounterState);
        let ty = extract_param_type(&arg);
        assert!(
            matches!(&ty, Type::Reference(TypeReference { elem, .. }) if matches!(elem.as_ref(), Type::Path(p) if p.qself.is_none() && p.path.is_ident("CounterState")))
        );
    }

    #[test]
    fn test_extract_param_type_from_self_receiver() {
        let arg: FnArg = parse_quote!(self);
        let ty = extract_param_type(&arg);
        assert!(matches!(&ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident("Self")));
    }

    #[test]
    fn test_extract_param_type_from_value_type() {
        let arg: FnArg = parse_quote!(x: i32);
        let ty = extract_param_type(&arg);
        assert!(matches!(&ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident("i32")));
    }

    // --- extract_state_type ---

    #[test]
    fn test_extract_state_type_from_mut_ref() {
        let inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> =
            parse_quote!(_: &mut CounterState, cx: &mut Context<CounterState>);
        let ty = extract_state_type(&inputs);
        assert!(matches!(&ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident("CounterState")));
    }

    #[test]
    fn test_extract_state_type_from_non_ref() {
        let inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> =
            parse_quote!(state: CounterState, cx: &mut Context<CounterState>);
        let ty = extract_state_type(&inputs);
        assert!(matches!(&ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident("CounterState")));
    }

    #[test]
    fn test_extract_state_type_from_empty_inputs_returns_self() {
        let inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> = parse_quote!();
        let ty = extract_state_type(&inputs);
        assert!(matches!(&ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident("Self")));
    }

    // --- is_unit ---

    #[test]
    fn test_is_unit_returns_true_for_empty_tuple() {
        let ty: Type = parse_quote!(());
        assert!(is_unit(&ty));
    }

    #[test]
    fn test_is_unit_returns_false_for_non_empty_tuple() {
        let ty: Type = parse_quote!((i32,));
        assert!(!is_unit(&ty));
    }

    #[test]
    fn test_is_unit_returns_false_for_other_types() {
        let ty: Type = parse_quote!(i32);
        assert!(!is_unit(&ty));
    }

    #[test]
    fn test_is_unit_returns_false_for_struct_type() {
        let ty: Type = parse_quote!(MyStruct);
        assert!(!is_unit(&ty));
    }

    // --- arg_count validation (contract) ---

    #[test]
    fn test_command_level_0_accepts_zero_args() {
        let item: ItemFn = parse_quote!(
            fn do_something() {
                println!("done");
            }
        );
        assert!(item.sig.inputs.is_empty());
        assert_eq!(item.sig.inputs.len(), 0);
    }

    #[test]
    fn test_command_level_2_accepts_one_arg() {
        let item: ItemFn = parse_quote!(
            fn handle_click(event: &ClickEvent) {
                let _ = event;
            }
        );
        assert_eq!(item.sig.inputs.len(), 1);
    }

    #[test]
    fn test_command_level_3_accepts_two_args() {
        let item: ItemFn = parse_quote!(
            fn update(state: &mut AppState, cx: &mut Context<AppState>) {
                let _ = (state, cx);
            }
        );
        assert_eq!(item.sig.inputs.len(), 2);
    }
}
