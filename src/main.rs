use warp::http;
use warp::{hyper::body::Bytes, Filter, Rejection};

async fn edit_and_perform_request(
    proxy_address: String,
    base_path: String,
    uri: warp::path::FullPath,
    params: Option<String>,
    method: http::Method,
    headers: http::HeaderMap,
    body: warp::hyper::body::Bytes,
) -> Result<http::Response<Bytes>, Rejection> {
    let headers = {
        use http::header::{HOST, REFERER};
        let mut headers = headers.clone();

        headers.remove(HOST);
        headers.insert(HOST, "pokemongo-get.com".parse().unwrap());

        if let Some(referrer) = headers.remove(REFERER) {
            let referrer = referrer.to_str().unwrap_or("");
            let referrer = referrer.replace("localhost:3030", "pokemongo-get.com");
            headers.insert(REFERER, referrer.parse().unwrap());
        }

        headers
    };

    let response = warp_reverse_proxy::proxy_to_and_forward_response(
        proxy_address,
        base_path,
        uri,
        params,
        method,
        headers,
        body,
    );

    match response.await {
        Ok(resp) => text_with_charset(resp).map_err(warp::reject::custom),
        Err(err) => Err(err),
    }
}

fn text_with_charset(
    response: http::Response<Bytes>,
) -> Result<warp::http::Response<bytes::Bytes>, warp_reverse_proxy::errors::Error> {
    use regex::bytes::{NoExpand, Regex};

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<mime::Mime>().ok());

    let encoding_name = content_type
        .as_ref()
        .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()));

    let body = match encoding_name {
        Some(_encoding_name) => {
            let regex = Regex::new(r"https?://pokemongo-get.com/").unwrap();
            let body = response.body();
            let body = regex.replace_all(&body, NoExpand(b"http://localhost:3030/"));

            bytes::Bytes::copy_from_slice(&body)
        }
        None => response.body().clone(),
    };

    let mut builder = http::Response::builder();
    for (k, v) in response.headers() {
        builder = builder.header(k, v);
    }
    return builder
        .status(response.status())
        .body(body)
        .map_err(warp_reverse_proxy::errors::Error::HTTP);
}

fn reverse_proxy_filter(
    base_path: String,
    proxy_address: String,
) -> impl Filter<Extract = (http::Response<Bytes>,), Error = Rejection> + Clone {
    let proxy_address = warp::any().map(move || proxy_address.clone());
    let base_path = warp::any().map(move || base_path.clone());
    let data_filter = warp_reverse_proxy::extract_request_data_filter();

    proxy_address
        .and(base_path)
        .and(data_filter)
        .and_then(edit_and_perform_request)
        .boxed()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let log = warp::log("ogipkmngo");

    let proxy = reverse_proxy_filter("".to_string(), "http://pokemongo-get.com/".to_string());
    let app = warp::any().and(proxy).with(log);

    // spawn proxy server
    warp::serve(app).run(([0, 0, 0, 0], 3030)).await;

    Ok(())
}
