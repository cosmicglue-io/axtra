extern crate self as axtra;

pub mod response {
    pub trait ResponseKey {
        fn response_key() -> &'static str;
    }
}

use axtra_macros::ResponseKey;
use response::ResponseKey as _;

#[derive(ResponseKey)]
struct GenericResponse<T>
where
    T: Clone,
{
    _value: T,
}

#[derive(ResponseKey)]
#[response_key = "custom"]
struct CustomResponse<T> {
    _value: T,
}

#[test]
fn derives_for_generic_models() {
    assert_eq!(
        GenericResponse::<String>::response_key(),
        "generic_response"
    );
    assert_eq!(CustomResponse::<String>::response_key(), "custom");
}
