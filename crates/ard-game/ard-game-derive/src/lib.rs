use proc_macro2::{Ident, Literal, Span};
use quote::{quote, ToTokens};
use syn::Fields;

#[proc_macro_derive(SaveLoad)]
pub fn save_load(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_save_load(&ast)
}

fn impl_save_load(ast: &syn::DeriveInput) -> proc_macro::TokenStream {
    let name = &ast.ident;

    let descriptor_name = Ident::new(
        &format!("{}Descriptor", &name.to_string()),
        Span::call_site(),
    );

    // Body of the descriptor object. Contains each field of the component as a descriptor.
    let descriptor_body = {
        let mut body = proc_macro2::TokenStream::default();

        match &ast.data {
            syn::Data::Struct(data) => {
                let (fields, is_struct) = gen_descriptor_fields(&data.fields);
                body.extend(if is_struct {
                    quote! {
                        { #fields }
                    }
                } else {
                    quote! {
                        ( #fields );
                    }
                });
            }
            syn::Data::Enum(data) => {
                for variant in &data.variants {
                    // Determine if the current variant is the default variant, so it can be marked
                    // as the default variant for the descriptor
                    let mut default_variant = false;
                    for attr in variant.attrs.iter() {
                        if attr.to_token_stream().to_string() == "#[default]" {
                            default_variant = true;
                            break;
                        }
                    }

                    // The fields of the enum
                    let variant_name = &variant.ident;
                    let fields = if variant.fields.is_empty() {
                        quote! {}
                    } else {
                        let (fields, is_struct) = gen_descriptor_fields(&variant.fields);
                        if is_struct {
                            quote! {
                                { #fields }
                            }
                        } else {
                            quote! {
                                ( #fields )
                            }
                        }
                    };

                    // Construct the variant
                    body.extend(if default_variant {
                        quote! {
                            #[default]
                            #variant_name #fields,
                        }
                    } else {
                        quote! {
                            #variant_name #fields,
                        }
                    });
                }

                body = quote! {
                    { #body }
                };
            }
            _ => unimplemented!("unions are not supported"),
        }

        body
    };

    // Body of the load function
    let load_body = {
        match &ast.data {
            syn::Data::Struct(data) => {
                let (body, _, is_struct) = gen_load_fields(&data.fields, false);

                if is_struct {
                    quote! {
                        Self { #body }
                    }
                } else {
                    quote! {
                        Self(#body)
                    }
                }
            }
            syn::Data::Enum(data) => {
                let mut body = proc_macro2::TokenStream::default();

                for variant in &data.variants {
                    let variant_ident = &variant.ident;
                    let (variant_body, field_names, is_struct) =
                        gen_load_fields(&variant.fields, true);

                    if is_struct {
                        let variant_body = quote! {
                            Self::#variant_ident {
                                #variant_body
                            }
                        };

                        body.extend(quote! {
                            #descriptor_name::#variant_ident {
                                #field_names
                            } => {
                                #variant_body
                            },
                        });
                    } else {
                        let variant_body = quote! {
                            Self::#variant_ident(
                                #variant_body
                            )
                        };

                        body.extend(quote! {
                            #descriptor_name::#variant_ident(
                                #field_names
                            ) => {
                                #variant_body
                            },
                        });
                    }
                }

                quote! {
                    match _descriptor {
                        #body
                    }
                }
            }
            _ => unimplemented!("unions are not supported"),
        }
    };

    // Body of the save function
    let save_body = {
        match &ast.data {
            syn::Data::Struct(data) => gen_save_fields(&data.fields, false).0,
            syn::Data::Enum(data) => {
                let mut body = proc_macro2::TokenStream::default();

                for variant in &data.variants {
                    let variant_ident = &variant.ident;
                    let (variant_body, fields, is_struct) = gen_save_fields(&variant.fields, true);

                    body.extend(if is_struct {
                        quote! {
                            Self::#variant_ident { #fields } => #descriptor_name::#variant_ident {
                                #variant_body
                            },
                        }
                    } else {
                        quote! {
                            Self::#variant_ident(#fields) => #descriptor_name::#variant_ident {
                                #variant_body
                            },
                        }
                    });
                }

                quote! {
                    descriptor = match self {
                        #body
                    };
                }
            }
            _ => unimplemented!("unions are not supported"),
        }
    };

    let desc_ty = match &ast.data {
        syn::Data::Struct(_) => proc_macro2::Ident::new("struct", Span::call_site()),
        syn::Data::Enum(_) => proc_macro2::Ident::new("enum", Span::call_site()),
        _ => unimplemented!("unions are not supported"),
    };

    let gen = quote! {
        impl SaveLoad for #name {
            type Descriptor = #descriptor_name;

            #[inline]
            fn save(
                &self,
                _entities: &crate::scene::EntityMap,
                _assets: &ard_assets::manager::Assets,
            ) -> Self::Descriptor {
                let mut descriptor = Self::Descriptor::default();
                #save_body
                descriptor
            }

            #[inline]
            fn load(
                _descriptor: Self::Descriptor,
                _entities: &crate::scene::EntityMap,
                _assets: &ard_assets::manager::Assets,
            ) -> Self {
                #load_body
            }
        }

        #[derive(Default, Serialize, Deserialize)]
        pub #desc_ty #descriptor_name #descriptor_body
    };

    gen.into()
}

/// Helper function to generate descriptor fields from a list of component fields.
fn gen_descriptor_fields(fields: &Fields) -> (proc_macro2::TokenStream, bool) {
    let mut body = proc_macro2::TokenStream::default();
    let mut is_struct = true;
    for field in fields.iter() {
        let ty = &field.ty;
        body.extend(match &field.ident {
            Some(ident) => quote! {
                #ident : <#ty as SaveLoad>::Descriptor,
            },
            None => {
                is_struct = false;
                quote! {
                    <#ty as SaveLoad>::Descriptor,
                }
            }
        })
    }
    (body, is_struct)
}

/// Helper function to generate the load function for fields of a descriptor. Returns the fields, a
/// comma separated list of fields names, and `true` if the fields are of a structure.
fn gen_load_fields(
    fields: &Fields,
    enum_impl: bool,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream, bool) {
    let mut is_struct = true;
    let mut body = proc_macro2::TokenStream::default();
    let mut field_names = proc_macro2::TokenStream::default();

    for (i, field) in fields.iter().enumerate() {
        let ty = &field.ty;
        body.extend(match &field.ident {
            Some(ident) => {
                field_names.extend(quote! { #ident, });

                let desc_ident = if enum_impl {
                    quote! {
                        #ident
                    }
                } else {
                    quote! {
                        _descriptor.#ident
                    }
                };

                quote! {
                    #ident : <#ty as SaveLoad>::load(#desc_ident, _entities, _assets),
                }
            }
            None => {
                is_struct = false;
                let ident = Ident::new(&format!("field_{}", i), Span::call_site());

                field_names.extend(quote! { #ident, });

                let desc_ident = if enum_impl {
                    quote! {
                        #ident
                    }
                } else {
                    let idx = Literal::usize_unsuffixed(i);
                    quote! {
                        _descriptor.#idx
                    }
                };

                quote! {
                    <#ty as SaveLoad>::load(#desc_ident, _entities, _assets),
                }
            }
        })
    }

    (body, field_names, is_struct)
}

fn gen_save_fields(
    fields: &Fields,
    enum_impl: bool,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream, bool) {
    let mut body = proc_macro2::TokenStream::default();
    let mut field_names = proc_macro2::TokenStream::default();
    let mut is_struct = true;
    let separator = if enum_impl {
        proc_macro2::Punct::new(':', proc_macro2::Spacing::Alone)
    } else {
        proc_macro2::Punct::new('=', proc_macro2::Spacing::Alone)
    };
    let suffix = if enum_impl {
        proc_macro2::Punct::new(',', proc_macro2::Spacing::Alone)
    } else {
        proc_macro2::Punct::new(';', proc_macro2::Spacing::Alone)
    };

    for (i, field) in fields.iter().enumerate() {
        body.extend(match &field.ident {
            Some(ident) => {
                field_names.extend(quote! {
                    #ident,
                });

                let desc_ident = if enum_impl {
                    quote! {
                        #ident
                    }
                } else {
                    quote! {
                        descriptor.#ident
                    }
                };

                let self_ident = if enum_impl {
                    quote! {
                        #ident
                    }
                } else {
                    quote! {
                        self.#ident
                    }
                };

                quote! {
                    #desc_ident #separator #self_ident.save(_entities, _assets) #suffix
                }
            }
            None => {
                is_struct = false;
                let ident = Literal::usize_unsuffixed(i);
                let desc_ident = Ident::new(&format!("field_{}", i), Span::call_site());

                field_names.extend(quote! {
                    #desc_ident,
                });

                let desc_ident = if enum_impl {
                    quote! {
                        #desc_ident
                    }
                } else {
                    quote! {
                        descriptor.#ident
                    }
                };

                quote! {
                    #desc_ident #separator self.#ident.save(_entities, _assets) #suffix
                }
            }
        })
    }

    (body, field_names, is_struct)
}
