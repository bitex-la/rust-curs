#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

extern crate curs;
extern crate http_stub;
extern crate serde;

use std::env;
use curs::hyper::header::{UserAgent, ContentType};
use curs::hyper::method::Method;
use curs::{Request, DecodableResult, CursResult, CursError, FileUpload};
use http_stub::HttpStub;
use http_stub as hs;

#[derive(Deserialize, Serialize, Debug, PartialEq)]
struct DummyJson {
    foo: String,
}

#[test]
fn successful_multipart() {
    let url = HttpStub::run(|mut stub| {
        stub.got_path("/some_post");
        stub.got_method(hs::Method::Post);
        stub.got_header("content-type", "multipart/form-data; boundary");
        stub.got_header("user-agent", "morcilla-firefox");
        stub.got_body(r"Content-Type: image/png");
        stub.got_body(r#"name="shim.png"; filename="test.png""#);
        stub.got_body(r#"Content-Disposition: form-data; name="two"\r\nvalue_two\r\n"#);

        stub.send_status(hs::StatusCode::Ok);
        stub.send_header(hs::header::ContentType(hs::Mime(hs::TopLevel::Application,
                                                          hs::SubLevel::Json,
                                                          vec![])));
        stub.send_body(r#"{"foo":"got files"}"#);
    });

    let file = FileUpload {
        name: "shim.png".to_string(),
        mime: None,
        path: &env::current_dir().unwrap().join("tests/fixtures/test.png"),
    };

    let response: DummyJson = Request::new(Method::Post, &*format!("{}/some_post", url))
                                  .params(vec![("one", "value_one"), ("two", "value_two")])
                                  .header(UserAgent("morcilla-firefox".to_string()))
                                  .files(vec![file])
                                  .send()
                                  .decode_success()
                                  .unwrap();
    assert_eq!(response, DummyJson { foo: "got files".to_string() });
}

#[test]
fn successful_json_get() {
    let url = HttpStub::run(|stub| {
        stub.got_path(r"/a_get\?one=value_one&two=value_two");
        stub.got_body("");
        stub.got_method(hs::Method::Get);
        stub.got_header("user-agent", "morcilla-firefox");
        stub.send_body(r#"{"foo":"bar"}"#);
    });

    let response: DummyJson = Request::new(Method::Get, &*format!("{}/a_get", url))
                                  .params(vec![("one", "value_one"), ("two", "value_two")])
                                  .header(UserAgent("morcilla-firefox".to_string()))
                                  .send()
                                  .decode_success()
                                  .unwrap();
    assert_eq!(response, DummyJson { foo: "bar".to_string() });
}

#[test]
fn successful_json_post() {
    let url = HttpStub::run(|stub| {
        stub.got_path("/some_post");
        stub.got_method(hs::Method::Post);
        stub.got_body("one=value_one&two=value_two");
        stub.send_body(r#"{"foo":"that"}"#);
    });

    let response: DummyJson = Request::new(Method::Post, &*format!("{}/some_post", url))
                                  .params(vec![("one", "value_one"), ("two", "value_two")])
                                  .send()
                                  .decode_success()
                                  .unwrap();
    assert_eq!(response, DummyJson { foo: "that".to_string() });
}

#[test]
fn successful_json_body_post() {
    let url = HttpStub::run(|stub| {
        stub.got_path("/see_this_json");
        stub.got_method(hs::Method::Post);
        stub.got_header("content-type", "application/json");
        stub.got_body(r#"\{"foo":"this"\}"#);
        stub.send_body(r#"{"foo":"that"}"#);
    });

    let response: DummyJson = Request::new(Method::Post, &*format!("{}/see_this_json", url))
    .json(DummyJson{ foo: "this".to_string() })
    // We still send params to make sure they don't get used but they dont break things.
    .params(vec![("one","value_one"), ("two", "value_two")])
    .send().decode_success().unwrap();
    assert_eq!(response, DummyJson { foo: "that".to_string() });
}

#[test]
fn successful_raw_body_post() {
    let url = HttpStub::run(|stub| {
        stub.got_path("/a_potato");
        stub.got_method(hs::Method::Post);
        stub.got_header("content-type", "application/potato");
        stub.got_body("A potato's body");
        stub.send_body(r#"{"foo":"potato"}"#);
    });

    let response: DummyJson = Request::new(Method::Post, &*format!("{}/a_potato", url))
                                  .header(ContentType("application/potato".parse().unwrap()))
                                  .override_body("A potato's body is delicious when fried"
                                                     .to_string())
                                  .send()
                                  .decode_success()
                                  .unwrap();
    assert_eq!(response, DummyJson { foo: "potato".to_string() });
}

#[test]
fn errors_out_with_not_found() {
    let url = HttpStub::run(|mut stub| {
        stub.got_body("");
        stub.got_method(hs::Method::Get);
        stub.send_status(hs::StatusCode::InternalServerError);
        stub.send_body("404 not found");
    });

    let result: CursResult<DummyJson> = Request::new(Method::Get, &*url).send().decode_success();

    match result.unwrap_err() {
        CursError::Status(_) => {}
        _ => panic!("No status error"),
    }
}
