// use reqwest;
use warp::http;
use warp::{http::Response, hyper::body::Bytes, Filter, Rejection, Reply};

async fn log_response(response: Response<Bytes>) -> Result<impl Reply, Rejection> {
    // println!("{:?}", response);
    Ok(response)
}

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
        use http::header::HOST;
        let mut headers = headers.clone();
        headers.remove(HOST);
        headers.insert(HOST, "pokemongo-get.com".parse().unwrap());
        headers
    };

    dbg!(&uri);
    dbg!(&headers);
    dbg!(&method);
    dbg!(&body);

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
    use encoding_rs::{Encoding, UTF_8};

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<mime::Mime>().ok());

    let encoding_name = content_type
        .as_ref()
        .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
        .unwrap_or("utf-8");

    let encoding = Encoding::for_label(encoding_name.as_bytes()).unwrap_or(UTF_8);

    let full = response.body();
    let (text, _, _) = encoding.decode(&full);
    let text = text.to_string();
    let body = bytes::Bytes::copy_from_slice(text.as_bytes());

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

    // Forward request to localhost in other port
    // let app = warp::path::full()
    //     .and(warp::query::raw())
    //     .map(move |path: FullPath, query| warp::reply::html(format!("{:?}?{:?}", path, query)))
    //     .with(log);

    let proxy = reverse_proxy_filter("".to_string(), "http://pokemongo-get.com/".to_string())
        .and_then(log_response);
    let app = warp::any().and(proxy).with(log);

    // spawn proxy server
    warp::serve(app).run(([0, 0, 0, 0], 3030)).await;

    Ok(())
}
