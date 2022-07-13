use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parenthesized,
    parse::{Parse, ParseStream, Result},
    parse_quote, Path,
};

#[proc_macro_derive(ComponentSerialization)]
pub fn resource(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_component_ser(&ast)
}

fn impl_component_ser(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let name_str = name.to_string();
    let gen = quote! {
        impl Resource for #name {
            fn debug_name() -> &'static str {
                #name_str
            }
        }
    };
    gen.into()
}
