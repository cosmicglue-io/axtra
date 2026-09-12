use inflector::Inflector;
use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{DeriveInput, Lit, Meta, parse_macro_input};

pub fn response_key_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_response_key(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_response_key(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let struct_name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    // Generate the default snake_case name once
    let default_key = struct_name.to_string().to_snake_case();

    // Look for the response_key attribute
    let response_key = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("response_key"))
        .map(|attribute| parse_response_key(attribute, &default_key))
        .transpose()?
        .unwrap_or(default_key);

    Ok(quote! {
        impl #impl_generics ::axtra::response::ResponseKey for #struct_name #type_generics #where_clause {
            fn response_key() -> &'static str {
                #response_key
            }
        }
    })
}

fn parse_response_key(attribute: &syn::Attribute, default_key: &str) -> syn::Result<String> {
    match &attribute.meta {
        Meta::Path(_) => Ok(default_key.to_string()),
        Meta::List(list) => match syn::parse2::<Lit>(list.tokens.clone())? {
            Lit::Str(value) => Ok(value.value()),
            literal => Err(syn::Error::new_spanned(
                literal,
                "response_key must be a string literal",
            )),
        },
        Meta::NameValue(name_value) => match &name_value.value {
            syn::Expr::Lit(expression) => match &expression.lit {
                Lit::Str(value) => Ok(value.value()),
                literal => Err(syn::Error::new_spanned(
                    literal,
                    "response_key must be a string literal",
                )),
            },
            expression => Err(syn::Error::new_spanned(
                expression.to_token_stream(),
                "response_key must be a string literal",
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_string_attribute_value() {
        let input: DeriveInput = syn::parse_quote! {
            #[response_key = 42]
            struct Response;
        };

        assert!(expand_response_key(input).is_err());
    }

    #[test]
    fn preserves_generics_and_where_clause() {
        let input: DeriveInput = syn::parse_quote! {
            struct Response<T> where T: Clone { value: T }
        };
        let output = expand_response_key(input).unwrap().to_string();

        assert!(output.contains("impl < T >"));
        assert!(output.contains("Response < T >"));
        assert!(output.contains("where T : Clone"));
    }
}
