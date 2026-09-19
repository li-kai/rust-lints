//! Source-local contracts consumed by the `rust-lints` Dylint library.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Item, LitStr, Result, Token, parse_macro_input};

struct ThreadAffineDropArgs {
    reason: LitStr,
}

impl Parse for ThreadAffineDropArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let name: Ident = input.parse()?;
        if name != "reason" {
            return Err(syn::Error::new(name.span(), "expected `reason`"));
        }
        input.parse::<Token![=]>()?;
        let reason: LitStr = input.parse()?;
        if reason.value().trim().is_empty() {
            return Err(syn::Error::new(
                reason.span(),
                "thread-affine drop contract requires a non-empty reason",
            ));
        }
        if !input.is_empty() {
            return Err(input.error("unexpected thread-affine drop contract argument"));
        }
        Ok(Self { reason })
    }
}

/// Declares that values of this type must be destroyed on a particular thread
/// or execution context.
///
/// The reason is embedded as hidden type metadata for the
/// `unsafe_send_thread_affine_drop` lint. The attribute does not implement or
/// change `Send`; the type should still be structurally `!Send` so every escape
/// remains an explicit `unsafe impl Send` audit point.
///
/// ```
/// use rust_lints_contracts::thread_affine_drop;
/// use std::marker::PhantomData;
/// use std::rc::Rc;
///
/// #[thread_affine_drop(reason = "must be released on the main thread")]
/// struct MainThreadHandle {
///     _not_send: PhantomData<Rc<()>>,
/// }
/// ```
#[proc_macro_attribute]
pub fn thread_affine_drop(args: TokenStream, item: TokenStream) -> TokenStream {
    let ThreadAffineDropArgs { reason } = parse_macro_input!(args as ThreadAffineDropArgs);
    let item = parse_macro_input!(item as Item);

    let (ident, generics) = match &item {
        Item::Struct(item) => (&item.ident, &item.generics),
        Item::Enum(item) => (&item.ident, &item.generics),
        _ => {
            return syn::Error::new_spanned(
                item,
                "`thread_affine_drop` can only be applied to a struct or enum",
            )
            .into_compile_error()
            .into();
        }
    };

    let marker = format_ident!("__RUST_LINTS_THREAD_AFFINE_DROP_CONTRACT");
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    quote! {
        #item

        impl #impl_generics #ident #type_generics #where_clause {
            #[doc(hidden)]
            const #marker: &'static str = #reason;
        }
    }
    .into()
}
