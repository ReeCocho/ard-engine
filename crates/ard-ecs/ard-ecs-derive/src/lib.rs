// Most of this code was ripped from specs-derive. I don't know how it works, but I don't need
// to for it to be useful :)

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parenthesized,
    parse::{Parse, ParseStream, Result},
    parse_quote, Path,
};

#[proc_macro_derive(Component)]
pub fn component(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_component(&ast)
}

fn impl_component(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let name_str = name.to_string();
    let gen = quote! {
        impl Component for #name {
            const NAME: &'static str = #name_str;
        }
    };
    gen.into()
}

struct StorageAttribute {
    storage: Path,
}

impl Parse for StorageAttribute {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        let _parenthesized_token = parenthesized!(content in input);

        Ok(StorageAttribute {
            storage: content.parse()?,
        })
    }
}

#[proc_macro_derive(Tag, attributes(storage))]
pub fn tag(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_tag(&ast)
}

fn impl_tag(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let name_str = name.to_string();

    let storage = ast
        .attrs
        .iter()
        .find(|attr| attr.path.segments[0].ident == "storage")
        .map(|attr| {
            syn::parse2::<StorageAttribute>(attr.tokens.clone())
                .unwrap()
                .storage
        })
        .unwrap_or_else(|| parse_quote!(UncommonStorage));

    let gen = quote! {
        impl Tag for #name {
            const NAME: &'static str = #name_str;
            type Storage = #storage<Self>;
        }
    };
    gen.into()
}

#[proc_macro_derive(Resource)]
pub fn resource(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_resource(&ast)
}

fn impl_resource(ast: &syn::DeriveInput) -> TokenStream {
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

#[proc_macro_derive(Event)]
pub fn event(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_event(&ast)
}

fn impl_event(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let name_str = name.to_string();
    let gen = quote! {
        impl Event for #name {
            const DEBUG_NAME: &'static str = #name_str;
        }
    };
    gen.into()
}

#[proc_macro_derive(SystemState)]
pub fn system_state(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_system_state(&ast)
}

fn impl_system_state(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let name_str = name.to_string();
    let gen = quote! {
        impl SystemState for #name {
            const DEBUG_NAME: &'static str = #name_str;
        }
    };
    gen.into()
}
