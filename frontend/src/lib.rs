use std::collections::HashMap;

use js_sys::wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

pub fn log(val: String) {
    web_sys::console::log_1(&val.into());
}
pub async fn request(
    method: &'static str,
    url: String,
    headers: HashMap<String, String>,
    body: Option<String>,
) -> Result<Response, ()> {
    let url: String = {
        let mut tmp: String = "http://127.0.0.1:3000".into();
        tmp.push_str(&url);
        tmp
    };
    let mut opts = RequestInit::new();
    opts.method(method);
    opts.mode(RequestMode::Cors);
    if let Some(body) = body {
        opts.body(Some(&body.into()));
    }
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|_| ())?;
    for (key, value) in headers {
        request.headers().set(&key, &value).map_err(|_| ())?;
    }
    let response = JsFuture::from(web_sys::window().ok_or(())?.fetch_with_request(&request))
        .await
        .map_err(|_| ())?
        .dyn_into::<Response>()
        .map_err(|_| ())?;
    Ok(response)
}
pub async fn get_text(response: Response) -> Result<String, ()> {
    JsFuture::from(response.text().map_err(|_| ())?)
        .await
        .map_err(|_| ())?
        .as_string()
        .ok_or(())
}
pub async fn get_json<T>(response: Response) -> Result<T, ()>
where
    for<'de> T: serde::de::Deserialize<'de>,
{
    serde_wasm_bindgen::from_value(
        JsFuture::from(response.json().map_err(|_| ())?)
            .await
            .map_err(|_| ())?,
    )
    .map_err(|_| ())
}
