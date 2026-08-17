use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, GenericParam, parse_macro_input, spanned::Spanned};

/// Derives the unsafe lifetime-rebranding proof used by `interner`.
///
/// The definition must have exactly one lifetime parameter and no where clause.
/// The generated identity reborrow compiles only when the definition is
/// covariant over that lifetime.
#[proc_macro_derive(Covariant)]
pub fn derive_covariant(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(output) => output,
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let mut parameters = input.generics.params.iter();
    let Some(GenericParam::Lifetime(_)) = parameters.next() else {
        return Err(syn::Error::new(
            input.generics.span(),
            "Covariant requires exactly one lifetime parameter",
        ));
    };
    if parameters.next().is_some() || input.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            input.generics.span(),
            "Covariant currently supports one lifetime parameter and no where clause",
        ));
    }

    let definition = input.ident;
    Ok(quote! {
        unsafe impl ::interner::Covariant for #definition<'static> {
            type Value<'a> = #definition<'a>;

            fn shorten<'long: 'short, 'short>(
                value: &'short Self::Value<'long>,
            ) -> &'short Self::Value<'short> {
                value
            }
        }
    }
    .into())
}
