use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{HeaderValue, ACCEPT_RANGES, CACHE_CONTROL, CONTENT_TYPE},
    Error, Result,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};

#[derive(Clone)]
pub struct HlsHeadersMiddleware;

impl<S, B> Transform<S, ServiceRequest> for HlsHeadersMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = HlsHeadersMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(HlsHeadersMiddlewareService { service }))
    }
}

pub struct HlsHeadersMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for HlsHeadersMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);

        Box::pin(async move {
            let mut res = fut.await?;

            let path = res.request().path();

            if path.ends_with(".m3u8") {
                res.headers_mut().insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/vnd.apple.mpegurl"),
                );
                res.headers_mut()
                    .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            } else if path.ends_with(".ts") {
                res.headers_mut()
                    .insert(CONTENT_TYPE, HeaderValue::from_static("video/MP2T"));
                res.headers_mut()
                    .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
                res.headers_mut()
                    .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            }

            // // CORS
            // res.headers_mut()
            //     .insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
            // res.headers_mut().insert(
            //     ACCESS_CONTROL_ALLOW_METHODS,
            //     HeaderValue::from_static("GET, POST, HEAD, OPTIONS"),
            // );
            // res.headers_mut().insert(
            //     ACCESS_CONTROL_ALLOW_HEADERS,
            //     HeaderValue::from_static("Range, Authorization"),
            // );

            Ok(res)
        })
    }
}
