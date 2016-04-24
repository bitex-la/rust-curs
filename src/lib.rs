//! Curs: Curl for rust users. Based on Hyper. File Uploads. JSON REST API Clients.
//!
//! Make your request, add params and files, or a JSON body, curs just works and decides the
//! right body format and adds any needed headers.
//!
//! Adding a raw request body is also possible, but if you need unopinionated flexibility
//! you can try falling back to hyper, re-exported as curs::hyper::client for your convenience.
//!
//! Then you can optionally deserialize json responses. Or just use them as text.
//!
//! [Fork on GitHub](https://github.com/bitex-la/rust-curs)
//!
//! #  Examples
//!
//! ```
//! // This example pretty much uses the whole library. Notice hyper is re-exported by curs.
//! // Uploads a file, and some params doing a POST request to a stubbed server.
//!
//! // The actual curs imports.
//! extern crate curs;
//! use curs::{Request, FileUpload, DecodableResult, Method};
//!
//! // Just stuff needed for this particular test.
//! extern crate http_stub;
//! use http_stub as hs;
//! use std::env;
//! use curs::hyper::header::UserAgent;
//!
//! fn main(){
//!  // Nevermind this stub HTTP server. Find the actual curs code below.
//!  let url = hs::HttpStub::run(|s|{ s.send_body(r#"["foo", "bar"]"#) });
//!
//!  let file = FileUpload{
//!    name: "shim.png".to_string(),
//!    mime: None,
//!    path: &env::current_dir().unwrap().join("tests/fixtures/test.png")};
//!
//!  let response : Vec<String> = Request::new(Method::Post, &*format!("{}/some_post", url))
//!    .params(vec![("one","value_one"), ("two", "value_two")])
//!    .header(UserAgent("morcilla-firefox".to_string()))
//!    .files(vec![file])
//!    .send().decode_success().unwrap();
//!  assert_eq!(response, vec!["foo", "bar"]);
//! }
//! ```

#![feature(custom_derive, plugin)]

pub extern crate serde;
pub extern crate serde_json;
pub extern crate hyper;
extern crate url;
extern crate rand;
extern crate mime_guess;

pub use self::hyper::method::Method;
pub use self::hyper::client::response::Response;
pub use self::hyper::status::StatusCode;

use std::path::Path;
use std::fs::File;
use std::io::Error as IoError;
use std::io::Read;
use self::rand::Rng;
use self::serde::{Deserialize, Serialize};
use self::hyper::header::{Headers, Header, HeaderFormat, ContentType};
use self::hyper::client::{Client, IntoUrl};
use self::hyper::error::Error as HyperError;
use self::hyper::mime::Mime;

/// Your result may be text or a struct deserialized from JSON.
/// The error is always a CursError
pub type CursResult<T> = Result<T, CursError>;

pub trait DecodableResult {
    fn decode_success<D: Deserialize>(self) -> CursResult<D>;
}

impl DecodableResult for CursResult<Response> {
    /// You can chain a decode_success call to your CursResult
    /// to deserialize a successful (2xx) JSON response. Using serde.
    fn decode_success<D: Deserialize>(self) -> CursResult<D> {
        let mut response = try!(self);
        match response.status {
            StatusCode::Ok | StatusCode::Created | StatusCode::Accepted => {
                let mut response_string = String::new();
                try!(response.read_to_string(&mut response_string));
                Ok(try!(serde_json::from_str(&response_string)))
            }
            _ => Err(CursError::Status(response)),
        }
    }
}

/// Sending your request may fail for any of the following reasons.
#[derive(Debug)]
pub enum CursError {
    Status(Response),
    Network(HyperError),
    Json(serde_json::Error),
}

impl From<HyperError> for CursError {
    fn from(err: HyperError) -> CursError {
        CursError::Network(err)
    }
}

impl From<IoError> for CursError {
    fn from(i: IoError) -> CursError {
        CursError::Network(HyperError::Io(i))
    }
}

impl From<serde_json::Error> for CursError {
    fn from(err: serde_json::Error) -> CursError {
        CursError::Json(err)
    }
}

/// All your params should go in a vector.
pub type Params<'a> = Vec<Param<'a>>;

/// And each param is just a &str tuple.
pub type Param<'a> = (&'a str, &'a str);

/// File uploads are more than just a path to a local file.
#[derive(Clone)]
pub struct FileUpload<'a> {
    pub name: String,
    pub mime: Option<Mime>,
    pub path: &'a Path,
}

/// You're not expected to be using the MultipartBodyBuilder on your own,
/// Make a Request and it will know when to delegate if there are any files
/// to be posted. It's still exported publicly because it may come in handy
/// for other uses.
pub struct MultipartBodyBuilder {
    body: Vec<u8>,
    boundary: String,
}

macro_rules! w {
  ($b:ident, $f:expr, $a: expr) => (
    $b.body.extend(format!($f, $a).as_bytes())
  )
}

impl MultipartBodyBuilder {
    pub fn new() -> MultipartBodyBuilder {
        let mut rng = rand::thread_rng();
        let boundary: String = rng.gen_ascii_chars().take(30).collect();
        MultipartBodyBuilder {
            body: vec![],
            boundary: boundary,
        }
    }

    pub fn build<'a>(mut self,
                     files: Vec<FileUpload>,
                     params: Params<'a>)
                     -> Result<MultipartBodyBuilder, CursError> {
        for (name, value) in params {
            w!(self, "\r\n--{}\r\n", self.boundary);
            w!(self, "Content-Disposition: form-data; name=\"{}\"", name);
            w!(self, "\r\n{}\r\n", value);
        }

        for FileUpload { name, path, mime } in files {
            w!(self, "\r\n--{}\r\n", self.boundary);
            w!(self, "Content-Disposition: form-data; name=\"{}\"", name);
            w!(self,
               "; filename=\"{}\"",
               path.file_name().unwrap().to_str().unwrap());
            w!(self,
               "\r\nContent-Type: {}\r\n\r\n",
               mime.unwrap_or_else(|| self::mime_guess::guess_mime_type(path)));

            let mut contents = try!(File::open(path));
            try!(contents.read_to_end(&mut self.body));
            self.body.extend("\r\n\r\n".as_bytes());
        }

        w!(self, "\r\n--{}--", self.boundary);

        Ok(self)
    }
}

/// The main entry point. Craft your request and send it.
#[derive(Clone)]
pub struct Request<'a> {
    method: Method,
    url: &'a str,
    params: Params<'a>,
    headers: Headers,
    files: Vec<FileUpload<'a>>,
    raw_body: Option<String>,
}

impl<'a> Request<'a> {
    /// You'll always need a method and the url to start.
    pub fn new(method: Method, url: &'a str) -> Request<'a> {
        Request {
            method: method,
            url: url,
            params: vec![],
            headers: Headers::new(),
            files: vec![],
            raw_body: None,
        }
    }

    /// Add params. This extends the existing params vector.
    pub fn params<P>(&mut self, additional: P) -> &mut Request<'a>
        where P: IntoIterator<Item = Param<'a>>
    {
        self.params.extend(additional);
        self
    }

    /// Use a serde::se::Serialize as JSON raw body.
    /// Adds the content-type: application/json header.
    /// This will override anything you've sent in in "params".
    /// If you need a json raw body *and* your params in the same request,
    /// either you're just being silly or need to fall back to curs::hyper::client.
    pub fn json<S: Serialize>(&mut self, thing: S) -> &mut Request<'a> {
        self.override_body(serde_json::to_string(&thing).unwrap());
        self.header(ContentType("application/json".parse().unwrap()));
        self
    }

    /// Sets a raw body, overriding anything that was previously set in params.
    /// Make sure to set the content-type header to match whatever you're adding here.
    pub fn override_body(&mut self, body: String) -> &mut Request<'a> {
        self.raw_body = Some(body);
        self
    }

    /// Add files to upload. This extends the existing files vector.
    pub fn files<F>(&mut self, additional: F) -> &mut Request<'a>
        where F: IntoIterator<Item = FileUpload<'a>>
    {
        self.files.extend(additional);
        self
    }

    /// Add a single header.
    pub fn header<H>(&mut self, additional: H) -> &mut Request<'a>
        where H: Header + HeaderFormat
    {
        self.headers.set(additional);
        self
    }

    /// Send your request and see what happens.
    pub fn send(&self) -> CursResult<Response> {
        let multipart_raw_body: Box<[u8]>; // We define it here for lifetime reasons.
        let params_as_query = &*url::form_urlencoded::serialize(&self.params);
        let mut url_string = self.url.into_url().unwrap().serialize();
        if self.params.len() > 0 && (self.method == Method::Get || self.method == Method::Head) {
            url_string = [&*url_string, "?", params_as_query].concat()
        }
        let client = Client::new();
        let mut request = client.request(self.method.clone(), &*url_string)
                                .headers(self.headers.clone());

        if let Some(ref body) = self.raw_body {
            request = request.body(&*body)
        } else {
            if self.method != Method::Get && self.method != Method::Head {
                request = if self.files.len() == 0 {
                    request.header(ContentType("application/x-www-form-urlencoded"
                                                   .parse()
                                                   .unwrap()))
                           .body(params_as_query)
                } else {
                    let builder = try!(MultipartBodyBuilder::new()
                                           .build(self.files.clone(), self.params.clone()));
                    let raw_mime = ["multipart/form-data; boundary=", &*builder.boundary].concat();
                    multipart_raw_body = builder.body.into_boxed_slice();
                    request.header(ContentType(raw_mime.parse().unwrap()))
                           .body(&*multipart_raw_body)
                }
            }
        }
        Ok(try!(request.send()))
    }
}
