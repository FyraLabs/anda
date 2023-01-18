use tracing::warn;

/// Exists for macros.rs
/// see gh `rpm-software-management/rpm` -> `rpmio/rpmstring.c`
///

/// Please use `format!()`.
pub(crate) fn rvasprintf() {
	todo!()
}
#[inline]
#[deprecated(note = "use c.is_uppercase() instead")]
pub(crate) fn risupper(c: char) -> bool {
	c >= 'A' && c <= 'Z'
}
#[inline]
#[deprecated(note = "use c.is_lowercase() instead")]
pub(crate) fn rislower(c: char) -> bool {
	c >= 'a' && c <= 'z'
}
#[inline]
#[deprecated(note = "use c.is_ascii_alphabetic() instead")]
pub(crate) fn risalpha(c: char) -> bool {
	rislower(c) || risupper(c)
}
