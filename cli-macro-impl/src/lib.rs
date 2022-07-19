use std::collections::BTreeMap;

use anyhow::Result;
use inflector::cases::{kebabcase::to_kebab_case, titlecase::to_title_case};
use openapitor::types::exts::{ParameterExt, ParameterSchemaOrContentExt, ReferenceOrExt, TokenStreamExt};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::Deserialize;
use serde_tokenstream::from_tokenstream;
use syn::ItemEnum;

/// The parameters passed to our macro.
#[derive(Deserialize, Debug)]
struct Params {
    /// The name of the tag that the commands are grouped buy.
    tag: String,
}

pub fn do_gen(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Get the data from the parameters.
    let params = from_tokenstream::<Params>(&attr)?;

    // Lets get the Open API spec.
    let api = openapitor::load_json_spec(include_str!("../../spec.json"))?;

    let ops = get_operations_with_tag(&api, &params.tag)?;

    let og_enum: ItemEnum = syn::parse2(item).unwrap();
    let mut variants = og_enum.variants.clone();
    let mut commands = quote!();

    // Let's iterate over the paths and generate the code.
    for op in ops {
        // Let's generate the delete command if it exists.
        if op.is_root_level_operation(&params.tag) && op.method == "DELETE" {
            let (delete_cmd, delete_enum_item) = op.generate_delete_command(&params.tag)?;

            commands = quote! {
                #commands

                #delete_cmd
            };

            // Clap with alphabetize the help text subcommands so it is fine to just shove
            // the variants on the end.
            variants.push(delete_enum_item);
        } else if op.is_root_level_operation(&params.tag) && op.method == "GET" {
            let (view_cmd, view_enum_item) = op.generate_view_command(&params.tag)?;

            commands = quote! {
                #commands

                #view_cmd
            };

            // Clap with alphabetize the help text subcommands so it is fine to just shove
            // the variants on the end.
            variants.push(view_enum_item);
        } else if op.is_root_level_operation(&params.tag) && op.method == "PUT" {
            let (edit_cmd, edit_enum_item) = op.generate_edit_command(&params.tag)?;

            commands = quote! {
                #commands

                #edit_cmd
            };

            // Clap with alphabetize the help text subcommands so it is fine to just shove
            // the variants on the end.
            variants.push(edit_enum_item);
        } else if op.is_root_create_operation(&params.tag) {
            let (create_cmd, create_enum_item) = op.generate_create_command(&params.tag)?;

            commands = quote! {
                #commands

                #create_cmd
            };

            // Clap with alphabetize the help text subcommands so it is fine to just shove
            // the variants on the end.
            variants.push(create_enum_item);
        } else if op.is_root_list_operation(&params.tag) {
            let (list_cmd, list_enum_item) = op.generate_list_command(&params.tag)?;

            commands = quote! {
                #commands

                #list_cmd
            };

            // Clap with alphabetize the help text subcommands so it is fine to just shove
            // the variants on the end.
            variants.push(list_enum_item);
        }
    }

    let attrs = og_enum.attrs;
    let code = quote!(
        use num_traits::identities::Zero;

        #(#attrs);*
        enum SubCommand {
            #variants
        }

        #commands
    );

    Ok(code)
}

struct Operation {
    op: openapiv3::Operation,
    method: String,
    path: String,
    id: String,
    spec: openapiv3::OpenAPI,
}

struct Property {
    schema: openapiv3::ReferenceOr<openapiv3::Schema>,
    required: bool,
    description: Option<String>,
}

struct Parameter {
    parameter: openapiv3::Parameter,
    required: bool,
}

impl Parameter {
    fn data(&self) -> Result<openapiv3::ParameterData> {
        (&self.parameter).data()
    }
}

impl Operation {
    /// Returns if the given operation is a root level operation on a specific tag.
    fn is_root_level_operation(&self, tag: &str) -> bool {
        let method = if self.method == "PUT" {
            "update".to_string()
        } else {
            self.method.to_lowercase()
        };

        (self.id.ends_with(&format!("{}_{}", method, singular(tag)))
            || self.id.ends_with(&format!("{}_{}_self", method, singular(tag))))
            && !self.op.tags.contains(&"hidden".to_string())
    }

    /// Returns if the given operation is a root list operation on a specific tag.
    fn is_root_list_operation(&self, tag: &str) -> bool {
        let pagination =
            if let Some(serde_json::value::Value::Bool(b)) = self.op.extensions.get("x-dropshot-pagination") {
                *b
            } else {
                return false;
            };

        self.id.ends_with(&format!("{}_{}", tag, self.method.to_lowercase())) && pagination && self.method == "GET"
    }

    /// Returns if the given operation is a root create operation on a specific tag.
    fn is_root_create_operation(&self, tag: &str) -> bool {
        self.id.ends_with(&format!("{}_{}", tag, self.method.to_lowercase())) && self.method == "POST"
    }

    fn get_parameters(&self) -> Result<BTreeMap<String, Parameter>> {
        let mut parameters = BTreeMap::new();

        for param in self.op.parameters.iter() {
            let param = param.item()?;

            let parameter_data = match param.data() {
                Ok(s) => s,
                Err(_) => return Ok(parameters),
            };

            parameters.insert(
                parameter_data.name.to_string(),
                Parameter {
                    parameter: param.clone(),
                    required: parameter_data.required,
                },
            );
        }

        Ok(parameters)
    }

    fn is_parameter(&self, parameter: &str) -> bool {
        for param in self.op.parameters.iter() {
            let param = match param.item() {
                Ok(i) => i,
                Err(_) => return false,
            };

            let parameter_data = match param.data() {
                Ok(s) => s,
                Err(_) => return false,
            };

            if parameter_data.name == parameter || parameter_data.name.starts_with(&format!("{}_", parameter)) {
                return true;
            }
        }

        false
    }

    fn get_request_body_name(&self) -> Result<String> {
        let request_body = match self.op.request_body.as_ref() {
            Some(r) => r,
            None => anyhow::bail!("no request_body found"),
        }
        .item()?;

        let content = match request_body.content.get("application/json") {
            Some(c) => c,
            None => anyhow::bail!("no `application/json` found"),
        };

        let schema = match content.schema.as_ref() {
            Some(s) => s,
            None => anyhow::bail!("no content schema found"),
        };

        schema.reference()
    }

    fn get_request_body_properties(&self) -> Result<BTreeMap<String, Property>> {
        let mut properties = BTreeMap::new();

        let request_body = match self.op.request_body.as_ref() {
            Some(r) => r,
            None => return Ok(properties),
        }
        .item()?;

        let content = match request_body.content.get("application/json") {
            Some(c) => c,
            None => return Ok(properties),
        };

        let schema = match content.schema.as_ref() {
            Some(s) => s,
            None => return Ok(properties),
        };

        let schema = match schema.item() {
            Ok(b) => b.clone(),
            Err(e) => {
                if e.to_string().contains("reference") {
                    schema.get_schema_from_reference(&self.spec, false)?
                } else {
                    anyhow::bail!("could not get schema from request body: {}", e);
                }
            }
        };

        let obj = match &schema.schema_kind {
            openapiv3::SchemaKind::Type(openapiv3::Type::Object(o)) => o,
            _ => return Ok(properties),
        };

        for (key, prop) in obj.properties.iter() {
            let mut key = key.to_string();

            let s = match prop.item() {
                Ok(s) => *s.clone(),
                Err(e) => {
                    if e.to_string().contains("reference") {
                        prop.get_schema_from_reference(&self.spec, false)?
                    } else {
                        anyhow::bail!("could not get schema from prop `{}`: {}", key, e);
                    }
                }
            };

            if self.method == "PUT" {
                // We add the `new_` part onto the parameter since it will be
                // overwriting an existing field.
                key = format!("new_{}", key);
            }

            properties.insert(
                key.clone(),
                Property {
                    schema: prop.clone().unbox(),
                    required: obj.required.contains(&key)
                        || obj.required.contains(&key.trim_start_matches("new_").to_string()),
                    description: s.schema_data.description,
                },
            );
        }

        Ok(properties)
    }

    /// Gets a list of all the string parameters for the operation.
    /// This includes the path parameters, query parameters, and request_body parameters.
    fn get_all_param_names(&self) -> Result<Vec<String>> {
        let mut param_names = Vec::new();

        for param in self.get_parameters()?.keys() {
            param_names.push(param.to_string());
        }

        for param in self.get_request_body_properties()?.keys() {
            param_names.push(param.to_string());
        }

        param_names.sort();

        Ok(param_names)
    }

    /// Gets all the api call params for the operation.
    /// This includes the path parameters, query parameters, and request_body parameters.
    fn get_api_call_params(&self, tag: &str) -> Result<Vec<TokenStream>> {
        let mut api_call_params: Vec<TokenStream> = Vec::new();

        let params = self.get_parameters()?;
        let mut params = params.keys().collect::<Vec<_>>();
        params.sort();

        for p in params {
            let mut p = p.to_string();

            if p == "page_token" {
                api_call_params.push(quote!(""));
                continue;
            }

            if p == "limit" {
                api_call_params.push(quote!(self.limit));
                continue;
            }

            p = clean_param_name(&p);

            let p = format_ident!("{}", p);

            if p == "sort_by" {
                // Sort by is an enum so we don't want to "&" it
                api_call_params.push(quote!(self.#p.clone()));
                continue;
            }

            api_call_params.push(quote!(&self.#p));
        }

        let req_body_properties = self.get_request_body_properties()?;
        if !req_body_properties.is_empty() {
            let mut req_body_rendered = Vec::new();
            for (p, v) in req_body_properties {
                let mut n = p.to_string();

                if self.method == "PUT" {
                    n = n.trim_start_matches("new_").to_string();
                }

                let p_og = format_ident!("{}", n);

                let mut new = if p == "id" { singular(tag) } else { p.to_string() };

                new = clean_param_name(&new);

                let p_short = format_ident!("{}", new);

                let type_name = self.render_type(v.schema, v.required)?;
                let rendered = type_name.rendered()?;

                if type_name.is_option()? && v.required {
                    // If the rendered property is an option, we want to unwrap it before
                    // sending the request since we were only doing that for the oneOf types.
                    // And we should only unwrap it if it is a required property.
                    if self.method == "PUT" {
                        req_body_rendered.push(quote!(#p_og: self.#p_short.as_ref().unwrap().clone()));
                    } else {
                        req_body_rendered.push(quote!(#p_og: #p_short.unwrap()));
                    }
                } else if rendered.starts_with("Vec<") {
                    // We parse all Vec's as strings and so now we have to convert them back to the
                    // original type.
                    req_body_rendered
                        .push(quote!(#p_og: self.#p_short.iter().map(|v| serde_json::from_str(v).unwrap()).collect()));
                } else if rendered == "uuid::Uuid" {
                    //if v.required {
                    req_body_rendered.push(quote!(#p_og: "".to_string()));
                } else if v.required {
                    req_body_rendered.push(quote!(#p_og: #p_short.clone()));
                } else if type_name.is_option()? && rendered != "kittycad::types::phone_number::PhoneNumber" {
                    // We can use self here since we aren't chaing the value from
                    // a prompt.
                    // In the future should we prompt for everything we would change this.
                    req_body_rendered.push(quote!(#p_og: Some(self.#p_short.clone())));
                } else {
                    req_body_rendered.push(quote!(#p_og: self.#p_short.clone()));
                }
            }

            let type_name = self.get_request_body_name()?;
            let type_name = format_ident!("{}", type_name);

            api_call_params.push(quote! {
                &kittycad::types::#type_name {
                    #(#req_body_rendered),*
                }
            });
        }

        Ok(api_call_params)
    }

    /// Gets a list of all the string parameters for the operation.
    /// This includes the path parameters, query parameters, and request_body parameters.
    #[allow(dead_code)]
    fn get_all_param_names_and_types(&self) -> Result<Vec<(String, openapiv3::ReferenceOr<openapiv3::Schema>)>> {
        let mut param_names = Vec::new();

        for (param, p) in self.get_parameters()? {
            let data = if let Ok(data) = p.data() {
                data
            } else {
                continue;
            };
            param_names.push((param.to_string(), data.format.schema()?));
        }

        for (param, p) in self.get_request_body_properties()? {
            param_names.push((param.to_string(), p.schema));
        }

        param_names.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(param_names)
    }

    /// Gets a list of all the required string parameters for the operation.
    /// This includes the path parameters, query parameters, and request_body parameters.
    fn get_all_required_param_names_and_types(
        &self,
    ) -> Result<Vec<(String, openapiv3::ReferenceOr<openapiv3::Schema>)>> {
        let mut param_names = Vec::new();

        for (param, p) in self.get_parameters()? {
            if p.data().unwrap().required {
                let data = if let Ok(data) = p.data() {
                    data
                } else {
                    continue;
                };
                param_names.push((param.to_string(), data.format.schema()?));
            }
        }

        for (param, p) in self.get_request_body_properties()? {
            if p.required {
                param_names.push((param.to_string(), p.schema));
            }
        }

        param_names.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(param_names)
    }

    fn render_struct_param(
        &self,
        name: &str,
        tag: &str,
        schema: openapiv3::ReferenceOr<openapiv3::Schema>,
        description: Option<String>,
        required: bool,
    ) -> Result<TokenStream> {
        if skip_defaults(name, tag)
            || name == format!("{}_name", singular(tag))
            || name == format!("{}_id", singular(tag))
            || name == "limit"
            || name == "page_token"
        {
            // Return early and empty, we don't care about these.
            return Ok(quote!());
        }

        let name_cleaned = clean_param_name(name);

        let name_ident = format_ident!("{}", name_cleaned);

        let n = if name_cleaned == "vpc" {
            name_cleaned.to_uppercase()
        } else {
            name_cleaned
        };

        let singular_tag = singular(tag);
        let prop = if singular_tag == "vpc" {
            singular_tag.to_uppercase()
        } else {
            singular_tag
        };

        let doc = if let Some(desc) = description {
            desc
        } else if name == "sort_by" {
            "The order in which to sort the results.".to_string()
        } else if name.starts_with("new_") {
            format!(
                "The new {} for the {}.",
                n.trim_start_matches("new_").replace('_', " "),
                prop
            )
            .replace(" dns ", " DNS ")
        } else if name == "description" {
            format!("The description for the {}.", prop)
        } else if self.is_root_list_operation(tag) {
            format!("The {} that holds the {}.", n, plural(&prop))
        } else {
            format!("The {} that holds the {}.", n, prop)
        };

        let mut type_name = self.render_type(schema, required)?;

        let rendered = type_name.rendered()?;

        let flags = get_flags(name)?;

        let short_flag = flags.get_short_token();
        let long_flag = flags.get_long_token();

        let requiredq = if required {
            quote!(true)
        } else if !rendered.starts_with("Option<") {
            // Default value is meaningless for Option types.
            quote!(false, default_value_t)
        } else {
            quote!(false)
        };

        if rendered.starts_with("Vec<") {
            type_name = quote!(Vec<String>);
        }

        let clap_line = if self.method == "POST" || name == "sort_by" {
            // On create, we want to set default values for the parameters.
            if rendered.starts_with("Option<") {
                // A default value there is pretty much always going to be None.
                quote! {
                    #[clap(#long_flag, #short_flag)]
                }
            } else if rendered.starts_with("Vec<") {
                // A default value there is pretty much always going to be None.
                quote! {
                    #[clap(#long_flag, #short_flag multiple_values = true)]
                }
            } else {
                quote! {
                    #[clap(#long_flag, #short_flag default_value_t)]
                }
            }
        } else {
            quote! {
                #[clap(#long_flag, #short_flag required = #requiredq)]
            }
        };

        Ok(quote! {
            #[doc = #doc]
            #clap_line
            pub #name_ident: #type_name,
        })
    }

    /// Get additional struct parameters.
    fn get_additional_struct_params(&self, tag: &str) -> Result<Vec<TokenStream>> {
        let mut params = Vec::new();

        for (param, p) in self.get_parameters()? {
            let data = if let Ok(data) = p.data() {
                data
            } else {
                continue;
            };

            // Let's get the type.
            let schema = data.format.schema()?;

            params.push(self.render_struct_param(&param, tag, schema, data.description, p.required)?);
        }

        for (param, p) in self.get_request_body_properties()? {
            params.push(self.render_struct_param(&param, tag, p.schema, p.description, p.required)?);
        }

        Ok(params)
    }

    /// Generate the create command.
    // TODO remove below once we have examples that need an extra prompt.
    #[allow(clippy::match_single_binding)]
    fn generate_create_command(&self, tag: &str) -> Result<(TokenStream, syn::Variant)> {
        let tag_ident = format_ident!("{}", tag);
        let singular_tag_str = singular(tag);
        let singular_tag_lc = format_ident!("{}", singular(tag));
        let struct_name = format_ident!("Cmd{}Create", to_title_case(&singular(tag)));

        let struct_doc = format!(
            "Create a new {}.\n\nTo create a {} interactively, use `kittycad {} create` with no arguments.",
            singular_tag_str,
            singular_tag_str,
            &singular(tag)
        );

        let struct_inner_name_doc = format!("The name of the {} to create.", singular_tag_str);

        let mut mutable_variables: Vec<TokenStream> = Vec::new();
        for (p, _) in self.get_all_required_param_names_and_types()? {
            let mut p = if p == "id" { singular(tag) } else { p };

            p = clean_param_name(&p);

            let ident = format_ident!("{}", p);

            mutable_variables.push(quote!(
                let mut #ident = self.#ident.clone();
            ));
        }

        let api_call_params = self.get_api_call_params(tag)?;

        let mut required_checks: Vec<TokenStream> = Vec::new();
        for (p, t) in self.get_all_required_param_names_and_types()? {
            let p = if p == "id" { singular(tag) } else { p };

            let n = clean_param_name(&p);

            let p = format_ident!("{}", n);

            let formatted = if n == singular(tag) {
                // Format like an argument not a flag.
                format!("[{}]", n)
            } else {
                let flags = get_flags(&n)?;
                flags.format_help()
            };

            let error_msg = format!("{} required in non-interactive mode", formatted);

            let is_check = self.get_is_check_fn(t, true)?;

            required_checks.push(quote!(
                if #p.#is_check() && !ctx.io.can_prompt() {
                    return Err(anyhow::anyhow!(#error_msg));
                }
            ));
        }

        let name_prompt = quote!(
            // Prompt for the resource name.
            if #singular_tag_lc.is_empty() {
                match dialoguer::Input::<String>::new()
                    .with_prompt(&format!("{} name:", #singular_tag_str))
                    .interact_text()
                {
                    Ok(name) => #singular_tag_lc = name,
                    Err(err) => {
                        return Err(anyhow::anyhow!("prompt failed: {}", err));
                    }
                }
            }
        );

        let mut additional_prompts: Vec<TokenStream> = Vec::new();
        for (p, v) in self.get_all_required_param_names_and_types()? {
            let n = clean_param_name(&p);

            if skip_defaults(&n, tag) {
                // Skip the prompt.
                continue;
            }

            let p = format_ident!("{}", n);

            let title = format!("{} {}", singular_tag_str, n);

            let is_check = self.get_is_check_fn(v.clone(), true)?;

            let rendered = self.render_type(v, true)?;
            let rendered_str = openapitor::types::get_text(&rendered)?
                .trim_start_matches("Option<")
                .trim_start_matches("kittycad::types::")
                .trim_end_matches('>')
                .to_string();
            let rendered = format_ident!("{}", rendered_str);

            let needs_extra_prompt: Option<(&str, bool)> = match rendered_str.as_str() {
                _ => None,
            };

            // Any weird OneOfs and other types that have a custom prompt should be
            // handled here.
            if let Some((base_prompt, is_optional)) = needs_extra_prompt {
                let prompt = if is_optional {
                    quote! { Some(kittycad::types::#rendered::prompt(#base_prompt)?) }
                } else {
                    quote! { kittycad::types::#rendered::prompt(#base_prompt)? }
                };
                additional_prompts.push(quote! {
                    // Prompt if they didn't provide the value.
                    if #p.#is_check() {
                        {
                            use crate::prompt_ext::PromptExt;
                            #p = #prompt;
                        }
                    }
                });

                // Continue through the loop early.
                continue;
            }

            additional_prompts.push(quote! {
                // Propmt if they didn't provide the value.
                if #p.#is_check() {
                    match dialoguer::Input::<_>::new()
                        .with_prompt(#title)
                        .interact_text()
                    {
                        Ok(input) => #p = input,
                        Err(err) => {
                            return Err(anyhow::anyhow!("prompt failed: {}", err));
                        }
                    }
                }
            });
        }

        // We need to form the output back to the client.
        let output = if self.is_parameter("organization") && (self.is_parameter("project") || tag == "projects") {
            let start = quote! {
                let full_name = format!("{}/{}", organization, project);
            };
            if tag != "projects" {
                quote! {
                    #start
                    writeln!(
                        ctx.io.out,
                        "{} Created {} {} in {}",
                        cs.success_icon(),
                        #singular_tag_str,
                        #singular_tag_lc,
                        full_name
                    )?;
                }
            } else {
                quote! {
                    #start
                    writeln!(
                        ctx.io.out,
                        "{} Created {} {}",
                        cs.success_icon(),
                        #singular_tag_str,
                        full_name
                    )?;
                }
            }
        } else {
            quote! {
                writeln!(
                    ctx.io.out,
                    "{} Created {} {}",
                    cs.success_icon(),
                    #singular_tag_str,
                    #singular_tag_lc
                )?;
            }
        };

        let additional_struct_params = self.get_additional_struct_params(tag)?;

        let cmd = quote!(
            #[doc = #struct_doc]
            #[derive(clap::Parser, Debug, Clone)]
            #[clap(verbatim_doc_comment)]
            pub struct #struct_name {
                #[doc = #struct_inner_name_doc]
                #[clap(name = #singular_tag_str, required = true)]
                pub #singular_tag_lc: String,

                #(#additional_struct_params)*
            }

            #[async_trait::async_trait]
            impl crate::cmd::Command for #struct_name {
                async fn run(&self, ctx: &mut crate::context::Context) -> anyhow::Result<()> {
                    #(#mutable_variables)*

                    #(#required_checks)*

                    let client = ctx.api_client("")?;

                    // Prompt for various parameters if we can, and the user passed them as empty.
                    if ctx.io.can_prompt() {
                        #name_prompt

                        #(#additional_prompts)*
                    }

                    client
                        .#tag_ident()
                        .post(
                            #(#api_call_params),*
                        )
                        .await?;

                    let cs = ctx.io.color_scheme();
                    #output

                    Ok(())
                }
            }
        );

        let enum_item: syn::Variant = syn::parse2(quote!(Create(#struct_name)))?;

        Ok((cmd, enum_item))
    }

    /// Generate the edit command.
    fn generate_edit_command(&self, tag: &str) -> Result<(TokenStream, syn::Variant)> {
        let tag_ident = format_ident!("{}", tag);
        let fn_name_ident = if tag == "users" {
            format_ident!("{}_self", "update")
        } else {
            format_ident!("{}", "update")
        };

        let singular_tag_str = singular(tag);
        let singular_tag_lc = format_ident!("{}", singular(tag));
        let struct_name = format_ident!("Cmd{}Edit", to_title_case(&singular(tag)));

        let struct_doc = format!("Edit {} settings.", singular_tag_str,);

        let struct_inner_name_doc = format!("The {} to edit. Can be an ID or name.", singular_tag_str);

        let api_call_params = self.get_api_call_params(tag)?;

        let mut check_nothing_to_edit = quote!(if);
        let mut i = 0;
        let req_body_properties = self.get_request_body_properties()?;
        for (p, v) in &req_body_properties {
            if skip_defaults(p, tag) {
                // Skip the defaults.
                continue;
            }

            let n = clean_param_name(p);

            let p = format_ident!("{}", n);

            let is_check = self.get_is_check_fn(v.schema.clone(), v.required)?;

            check_nothing_to_edit = quote! {
                #check_nothing_to_edit self.#p.#is_check()
            };

            if i < req_body_properties.len() - 1 {
                // Add the && if we need it.
                check_nothing_to_edit = quote! {
                    #check_nothing_to_edit &&
                };
            } else {
                check_nothing_to_edit = quote! {
                    #check_nothing_to_edit {
                        return Err(anyhow::anyhow!("nothing to edit"));
                    }
                };
            }

            i += 1;
        }

        // We need to form the output back to the client.
        let output = quote! {
                writeln!(
                    ctx.io.out,
                    "{} Edited {}",
                    cs.success_icon_with_color(ansi_term::Color::Red),
                    #singular_tag_str,
                )?;
        };

        let additional_struct_params = self.get_additional_struct_params(tag)?;

        let params = if tag == "users" {
            quote!()
        } else {
            quote!(

                #[doc = #struct_inner_name_doc]
                #[clap(name = #singular_tag_str, required = true)]
                pub #singular_tag_lc: String,
            )
        };

        let cmd = quote!(
            #[doc = #struct_doc]
            #[derive(clap::Parser, Debug, Clone)]
            #[clap(verbatim_doc_comment)]
            pub struct #struct_name {
                #params

                #(#additional_struct_params)*
            }

            #[async_trait::async_trait]
            impl crate::cmd::Command for #struct_name {
                async fn run(&self, ctx: &mut crate::context::Context) -> anyhow::Result<()> {
                    #check_nothing_to_edit

                    let client = ctx.api_client("")?;

                    let result = client.#tag_ident().#fn_name_ident(#(#api_call_params),*).await?;

                    let cs = ctx.io.color_scheme();
                    #output

                    Ok(())
                }
            }
        );

        let enum_item: syn::Variant = syn::parse2(quote!(
                Edit(#struct_name)
        ))?;

        Ok((cmd, enum_item))
    }

    /// Generate the view command.
    fn generate_view_command(&self, tag: &str) -> Result<(TokenStream, syn::Variant)> {
        let tag_ident = format_ident!("{}", tag);
        let fn_name_ident = if tag == "users" {
            format_ident!("{}_self", "get")
        } else {
            format_ident!("{}", "get")
        };

        let singular_tag_str = singular(tag);
        let singular_tag_lc = format_ident!("{}", singular(tag));
        let struct_name = format_ident!("Cmd{}View", to_title_case(&singular(tag)));

        let struct_doc = format!(
            "View {}.\n\nDisplay information about a KittyCAD {}.\n\nWith `--web`, open the {} in a web browser instead.",
            singular_tag_str, singular_tag_str, singular_tag_str
        );

        let struct_inner_web_doc = format!("Open the {} in the browser.", singular_tag_str);
        let struct_inner_name_doc = format!("The {} to view. Can be an ID or name.", singular_tag_str);

        let api_call_params = self.get_api_call_params(tag)?;

        let additional_struct_params = self.get_additional_struct_params(tag)?;

        let params = if tag == "users" {
            quote!()
        } else {
            quote!(

                #[doc = #struct_inner_name_doc]
                #[clap(name = #singular_tag_str, required = true)]
                pub #singular_tag_lc: String,
            )
        };

        let url = if tag == "users" {
            quote!(
                    let url = "https://kittycad.io/account".to_string();
            )
        } else {
            let path = &self.path;
            quote!(
                    let url = format!(
                        "https://{}{}",
                        ctx.config.default_host()?,
                        #path
                    );
            )
        };

        let format_flag = format_flag();

        let cmd = quote!(
            #[doc = #struct_doc]
            #[derive(clap::Parser, Debug, Clone)]
            #[clap(verbatim_doc_comment)]
            pub struct #struct_name {
                #params

                #(#additional_struct_params)*

                #[doc = #struct_inner_web_doc]
                #[clap(short, long)]
                pub web: bool,

                #format_flag
            }

            #[async_trait::async_trait]
            impl crate::cmd::Command for #struct_name {
                async fn run(&self, ctx: &mut crate::context::Context) -> anyhow::Result<()> {
                    if self.web {
                        #url

                        ctx.browser("", &url)?;
                        return Ok(());
                    }

                    let client = ctx.api_client("")?;

                    let result = client.#tag_ident().#fn_name_ident(#(#api_call_params),*).await?;

                    let format = ctx.format(&self.format)?;
                    ctx.io.write_output(&format, &result)?;
                    Ok(())
                }
            }
        );

        let enum_item: syn::Variant = syn::parse2(quote!(
                #[clap(alias = "get")]
                View(#struct_name)
        ))?;

        Ok((cmd, enum_item))
    }

    /// Generate the list command.
    fn generate_list_command(&self, tag: &str) -> Result<(TokenStream, syn::Variant)> {
        let tag_ident = format_ident!("{}", tag);
        let singular_tag_str = singular(tag);
        let struct_name = format_ident!("Cmd{}List", to_title_case(&singular(tag)));

        let struct_doc = format!("List {}.", plural(&singular_tag_str));

        let api_call_params = self.get_api_call_params(tag)?;

        let mut api_call_params_all: Vec<TokenStream> = Vec::new();
        for p in self.get_all_param_names()? {
            if p == "limit" || p == "page_token" {
                continue;
            }

            if p == "sort_by" {
                api_call_params_all.push(quote!(self.sort_by.clone()));
                continue;
            }

            let n = clean_param_name(&p);
            let ident = format_ident!("{}", n);

            api_call_params_all.push(quote!(&self.#ident));
        }

        let additional_struct_params = self.get_additional_struct_params(tag)?;

        let format_flag = format_flag();
        let cmd = quote!(
            #[doc = #struct_doc]
            #[derive(clap::Parser, Debug, Clone)]
            #[clap(verbatim_doc_comment)]
            pub struct #struct_name {
                #(#additional_struct_params)*

                /// Maximum number of items to list.
                #[clap(long, short, default_value = "30")]
                pub limit: u32,

                /// Make additional HTTP requests to fetch all pages.
                #[clap(long)]
                pub paginate: bool,

                #format_flag
            }

            #[async_trait::async_trait]
            impl crate::cmd::Command for #struct_name {
                async fn run(&self, ctx: &mut crate::context::Context) -> anyhow::Result<()> {
                    if self.limit < 1 {
                    return Err(anyhow::anyhow!("--limit must be greater than 0"));
                }

                let client = ctx.api_client("")?;

                let results = if self.paginate {
                    client
                        .#tag_ident()
                        .get_all(
                            #(#api_call_params_all),*
                        )
                        .await?
                } else {
                    client
                        .#tag_ident()
                        .get_page(
                            #(#api_call_params),*
                        )
                        .await?
                };

                let format = ctx.format(&self.format)?;
                ctx.io.write_output_for_vec(&format, &results)?;
                Ok(())
            }
        }
        );

        let enum_item: syn::Variant = syn::parse2(quote!(List(#struct_name)))?;

        Ok((cmd, enum_item))
    }

    /// Generate the delete command.
    fn generate_delete_command(&self, tag: &str) -> Result<(TokenStream, syn::Variant)> {
        let tag_ident = format_ident!("{}", tag);
        let fn_name_ident = if tag == "users" {
            format_ident!("{}_self", "delete")
        } else {
            format_ident!("{}", "delete")
        };

        let singular_tag_str = singular(tag);
        let singular_tag_lc = format_ident!("{}", singular(tag));
        let struct_name = format_ident!("Cmd{}Delete", to_title_case(&singular(tag)));

        let struct_doc = format!("Delete {}.", singular_tag_str);
        let struct_inner_name_doc = format!("The {} to delete. Can be an ID or name.", singular_tag_str);

        let api_call_params = self.get_api_call_params(tag)?;

        let additional_struct_params = self.get_additional_struct_params(tag)?;

        // We need to form the output back to the client.
        let output = if self.is_parameter("organization") && self.is_parameter("project") {
            let start = quote! {
                let full_name = format!("{}/{}", self.organization, self.project);
            };
            if tag != "projects" {
                quote! {
                    #start
                    writeln!(
                        ctx.io.out,
                        "{} Deleted {} {} from {}",
                        cs.success_icon_with_color(ansi_term::Color::Red),
                        #singular_tag_str,
                        self.#singular_tag_lc,
                        full_name
                    )?;
                }
            } else {
                quote! {
                    #start
                    writeln!(
                        ctx.io.out,
                        "{} Deleted {} {}",
                        cs.success_icon_with_color(ansi_term::Color::Red),
                        #singular_tag_str,
                        full_name
                    )?;
                }
            }
        } else {
            quote! {
                writeln!(
                    ctx.io.out,
                    "{} Deleted {} {}",
                    cs.success_icon_with_color(ansi_term::Color::Red),
                    #singular_tag_str,
                    self.#singular_tag_lc
                )?;
            }
        };

        let cmd = quote!(
            #[doc = #struct_doc]
            #[derive(clap::Parser, Debug, Clone)]
            #[clap(verbatim_doc_comment)]
            pub struct #struct_name {
                #[doc = #struct_inner_name_doc]
                #[clap(name = #singular_tag_str, required = true)]
                pub #singular_tag_lc: String,

                #(#additional_struct_params)*

                /// Confirm deletion without prompting.
                #[clap(long)]
                pub confirm: bool,
            }

            #[async_trait::async_trait]
            impl crate::cmd::Command for #struct_name {
                async fn run(&self, ctx: &mut crate::context::Context) -> anyhow::Result<()> {
                    if !ctx.io.can_prompt() && !self.confirm {
                        return Err(anyhow::anyhow!("--confirm required when not running interactively"));
                    }

                    let client = ctx.api_client("")?;


                    // Confirm deletion.
                    if !self.confirm {
                        if let Err(err) = dialoguer::Input::<String>::new()
                            .with_prompt(format!("Type {} to confirm deletion:", self.#singular_tag_lc))
                            .validate_with(|input: &String| -> Result<(), &str> {
                                if input.trim() == self.#singular_tag_lc {
                                    Ok(())
                                } else {
                                    Err("mismatched confirmation")
                                }
                            })
                            .interact_text()
                        {
                            return Err(anyhow::anyhow!("prompt failed: {}", err));
                        }
                    }


                    client
                        .#tag_ident()
                        .#fn_name_ident(#(#api_call_params),*)
                        .await?;

                    let cs = ctx.io.color_scheme();

                    #output

                    Ok(())
                }
            }
        );

        let enum_item: syn::Variant = syn::parse2(quote!(Delete(#struct_name)))?;

        Ok((cmd, enum_item))
    }

    fn render_type(
        &self,
        s: openapiv3::ReferenceOr<openapiv3::Schema>,
        required: bool,
    ) -> Result<proc_macro2::TokenStream> {
        let mut t = match &s {
            openapiv3::ReferenceOr::Reference { .. } => {
                openapitor::types::get_type_name_from_reference(&s.reference()?, &self.spec, false)?
            }
            openapiv3::ReferenceOr::Item(s) => openapitor::types::get_type_name_for_schema("", s, &self.spec, false)?,
        };

        if !required && !t.is_option()? {
            t = quote!(Option<#t>)
        }

        let fix = t.rendered()?.replace("crate::types::", "kittycad::types::");

        fix.parse().map_err(|err| anyhow::anyhow!("{}", err))
    }

    fn get_is_check_fn(
        &self,
        s: openapiv3::ReferenceOr<openapiv3::Schema>,
        required: bool,
    ) -> Result<proc_macro2::Ident> {
        let rendered = &self.render_type(s.clone(), required)?;

        let ident = if rendered.is_option()? {
            format_ident!("{}", "is_none")
        } else {
            let rendered = match s.get_schema_from_reference(&self.spec, true) {
                Ok(s) => openapitor::types::get_type_name_for_schema("", &s, &self.spec, true)?.rendered()?,
                Err(_) => rendered.to_string(),
            };

            if rendered.starts_with('u') || rendered.starts_with('i') || rendered.starts_with('f') {
                // Handle numbers.
                format_ident!("{}", "is_zero")
            } else {
                format_ident!("{}", "is_empty")
            }
        };

        Ok(ident)
    }
}

/// Get the operations with the tag from the OpenAPI spec.
fn get_operations_with_tag(api: &openapiv3::OpenAPI, tag: &str) -> Result<Vec<Operation>> {
    let mut paths = Vec::new();

    for (pn, p) in api.paths.iter() {
        let op = p.item()?;

        let grab = |pn: &str, m: &str, o: Option<&openapiv3::Operation>| -> Result<Vec<Operation>> {
            if let Some(o) = o {
                for t in o.tags.iter() {
                    if t == tag {
                        let id = if let Some(i) = o.operation_id.as_ref() {
                            i.to_string()
                        } else {
                            "".to_string()
                        };

                        return Ok(vec![Operation {
                            op: o.clone(),
                            method: m.to_string(),
                            path: pn.to_string(),
                            id,
                            spec: api.clone(),
                        }]);
                    }
                }
            }

            Ok(Default::default())
        };

        paths.append(&mut grab(pn, "GET", op.get.as_ref())?);
        paths.append(&mut grab(pn, "POST", op.post.as_ref())?);
        paths.append(&mut grab(pn, "PUT", op.put.as_ref())?);
        paths.append(&mut grab(pn, "DELETE", op.delete.as_ref())?);
        paths.append(&mut grab(pn, "OPTIONS", op.options.as_ref())?);
        paths.append(&mut grab(pn, "HEAD", op.head.as_ref())?);
        paths.append(&mut grab(pn, "PATCH", op.patch.as_ref())?);
        paths.append(&mut grab(pn, "TRACE", op.trace.as_ref())?);
    }

    Ok(paths)
}

/// Return the plural version of a string.
fn plural(s: &str) -> String {
    let s = singular(s);

    if s.ends_with('s') {
        return format!("{}es", s);
    } else if s.ends_with('y') {
        return format!("{}ies", s.trim_end_matches('y'));
    }

    format!("{}s", s)
}

/// Return the singular version of a string (if it plural).
fn singular(s: &str) -> String {
    if let Some(b) = s.strip_suffix('s') {
        return b.to_string();
    }

    s.to_string()
}

fn skip_defaults(n: &str, tag: &str) -> bool {
    n == singular(tag) || n == "id"
}

fn clean_param_name(p: &str) -> String {
    p.to_string()
}

struct Flags {
    short: char,
    long: String,
}

impl Flags {
    fn format_help(&self) -> String {
        if self.short != '0' {
            format!("-{}|--{}", self.short, self.long)
        } else {
            format!("--{}", self.long)
        }
    }

    fn get_short_token(&self) -> TokenStream {
        if self.short != '0' {
            let c = self.short;
            quote!(short = #c,)
        } else {
            quote!()
        }
    }

    fn get_long_token(&self) -> TokenStream {
        let mut l = self.long.to_string();
        if l == "ipv-6-prefix" {
            l = "ipv6-prefix".to_string();
        } else if l == "ipv-4-prefix" {
            l = "ipv4-prefix".to_string();
        }
        quote!(long = #l)
    }
}

fn get_flags(name: &str) -> Result<Flags> {
    if name.len() < 2 {
        anyhow::bail!("name must be at least 2 characters long");
    }

    // Remove the new_prefix we added to the start of the name. Since not everything can
    // have an 'n' short flag.
    let name = name.trim_start_matches("new_");

    let mut long = to_kebab_case(name).replace("ipv-4", "ipv4").replace("ipv-6", "ipv6");

    if long == "vpc-name" || long == "router-name" {
        long = long.trim_end_matches("-name").to_string();
    }

    let mut flags = Flags {
        short: name.to_lowercase().chars().next().unwrap(),
        long,
    };

    // TODO: we should smartly parse the flags and make sure there is no overlap.
    if name == "description" {
        flags.short = flags.short.to_ascii_uppercase();
    } else if name == "size" || flags.short == 'd' || flags.short == 'h' {
        // 'd' is debug, 'h' is help
        flags.short = '0';
    } else if name == "ncpus" {
        flags.short = 'c';
    } else if flags.long == "ipv4-block" {
        flags.short = '4';
    } else if flags.long == "ipv6-block" {
        flags.short = '6';
    }

    Ok(flags)
}

fn format_flag() -> TokenStream {
    quote!(
        /// Output format.
        #[clap(long, short, arg_enum)]
        pub format: Option<crate::types::FormatOutput>,
    )
}
