use cli_macro_impl::do_gen;
use quote::quote;

#[test]
fn test_do_gen() {
    let mut actual = do_gen(
        quote! {
            tag = "users",
        },
        quote! {
            #[derive(Parser, Debug, Clone)]
            enum SubCommand {
            }
        },
    )
    .unwrap();

    expectorate::assert_contents(
        "tests/gen/users.rs.gen",
        &openapitor::types::get_text_fmt(&actual).unwrap(),
    );

    actual = do_gen(
        quote! {
            tag = "api-calls",
        },
        quote! {
            #[derive(Parser, Debug, Clone)]
            enum SubCommand {}
        },
    )
    .unwrap();

    expectorate::assert_contents(
        "tests/gen/api-calls.rs.gen",
        &openapitor::types::get_text_fmt(&actual).unwrap(),
    );
}
