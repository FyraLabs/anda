use http_auth_basic::Credentials;
use ldap3::{LdapConnAsync, SearchEntry};
use rocket::{request::{self, Request, FromRequest}, outcome::Outcome, http::Status};

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum UserError {
    Missing,
    Invalid,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = UserError;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let base_uri = std::env::var("LDAP_URI").unwrap();
        let base_dn = std::env::var("LDAP_BASE_DN").unwrap();

        let auth_header = match req.headers().get_one("Authorization") {
            Some(auth_header) => auth_header,
            None => return Outcome::Failure((Status::Unauthorized, UserError::Missing)),
        };

        let credentials = match Credentials::from_header(auth_header.to_owned()) {
            Ok(credentials) => credentials,
            Err(_) => return Outcome::Failure((Status::BadRequest, UserError::Invalid)),
        };

        let (conn, mut ldap) = match LdapConnAsync::new(&base_uri).await {
            Ok(conn) => conn,
            Err(_) => return Outcome::Failure((Status::InternalServerError, UserError::Invalid)),
        };
        ldap3::drive!(conn);

        let user_dn = format!("cn={},{}", credentials.user_id, base_dn);

        let res = match ldap.simple_bind(&user_dn, &credentials.password).await {
            Ok(r) => r,
            Err(_) => return Outcome::Failure((Status::InternalServerError, UserError::Invalid)),
        };

        match res.success() {
            Ok(_) => {},
            Err(_) => return Outcome::Failure((Status::Unauthorized, UserError::Invalid)),
        };

        let res = match ldap.compare(&user_dn, "userPassword", &credentials.password).await {
            Ok(r) => r.equal(),
            Err(_) => return Outcome::Failure((Status::InternalServerError, UserError::Invalid)),
        };

        match res {
            Ok(true) => {},
            _ => return Outcome::Failure((Status::Unauthorized, UserError::Invalid)),
        };

        let res = match ldap.search(&base_dn, ldap3::Scope::OneLevel, &format!("cn={}", credentials.user_id), vec!["uid", "name"]).await {
            Ok(r) => r.0,
            Err(_) => return Outcome::Failure((Status::InternalServerError, UserError::Invalid)),
        };

        let entry = match res.first() {
            Some(e) => e,
            None => return Outcome::Failure((Status::Unauthorized, UserError::Invalid)),
        };

        let entry = SearchEntry::construct(entry.clone());

        let uid = match entry.attrs.get("uid") {
            Some(uid) => uid,
            None => return Outcome::Failure((Status::Unauthorized, UserError::Invalid)),
        };
        let name = match entry.attrs.get("name") {
            Some(name) => name,
            None => return Outcome::Failure((Status::Unauthorized, UserError::Invalid)),
        };


        let user = User {
            id: uid[0].to_string(),
            username: credentials.user_id,
            name: name[0].to_string(),
        };

        Outcome::Success(user)
    }
}