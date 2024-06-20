use super::*;

#[test]
fn uri_trait_test() {
    let uri = anda_uri::<PathUri>("/home/user/file.txt").unwrap();
    // println!("{}", uri.to_string_uri());

    assert_eq!(uri.to_string_uri(), "file:///home/user/file.txt");

    // Test if it's already a URI
    let uri = anda_uri::<PathUri>("file:///home/user/file.txt").unwrap();
    // println!("{}", uri.to_string_uri());

    assert_eq!(uri.to_string_uri(), "file:///home/user/file.txt");

    let uri = anda_uri::<forge::GitHubUri>("github:rust-lang/cargo").unwrap();
    println!("{}", uri.to_string_uri());
    assert_eq!(uri.to_string_uri(), "git+https://github.com/rust-lang/cargo");

    let uri = anda_uri::<forge::PagureUri>("pagure:fedora/rust").unwrap();
    println!("{}", uri.to_string_uri());
    assert_eq!(uri.to_string_uri(), "git+https://pagure.io/fedora/rust");

    let uri = anda_uri::<forge::GitLabUri>("gitlab:fedora/rust").unwrap();
    println!("{}", uri.to_string_uri());
    assert_eq!(uri.to_string_uri(), "git+https://gitlab.com/fedora/rust");
}
