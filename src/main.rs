use actix_web::{web, App, HttpResponse, HttpServer, Responder, Result};
use reqwest::{header::{HeaderMap, HeaderValue, USER_AGENT}, Client};
use serde::{Deserialize, Serialize};
use tera::{Context, Tera};
use regex::Regex;
use std::{sync::Arc};
use tokio::sync::Mutex;
use lazy_static::lazy_static;

// iPadのUAに偽装
const USERAGENT: &str = "Mozilla/5.0 (iPad; CPU OS 6_0 like Mac OS X) AppleWebKit/536.26 (KHTML, like Gecko) Version/6.0 Mobile/10A403 Safari/8536.25";
const OPEN2CH_HEADLINECGI: &str = "https://hayabusa.open2ch.net/headline.cgi?bbs=livejupiter";

#[derive(Deserialize, Serialize, Clone)]
struct ThreadData {
    title: String,
    thrurl: String,
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String,
}

lazy_static! {
    static ref THREAD_REGEX: Regex = Regex::new("https://hayabusa.open2ch.net/test/read.cgi/livejupiter/([0-9]+)/l50").unwrap();
    static ref TITLE_REGEX: Regex = Regex::new("<title>(.+)</title>").unwrap();
}

///////////////////////////////////////////////
// START                                      //
////////////////////////////////////////////////

async fn index(tera: web::Data<Tera>) -> impl Responder {
    let ctx = Context::new();
    let html = tera.render("index.html", &ctx).unwrap_or_else(|_| "ERROR".into());
    HttpResponse::Ok().body(html)
}

async fn scan(
    tera: web::Data<Tera>,
    query: web::Query<SearchQuery>,
    client: web::Data<Client>,
) -> impl Responder {
    let response = client.get(OPEN2CH_HEADLINECGI)
        .header(USER_AGENT, USERAGENT)
        .send().await;

    let thread_list_html = match response {
        Ok(resp) => match resp.text().await {
            Ok(text) => text,
            Err(_) => return HttpResponse::InternalServerError().body("Failed to get thread list text"),
        },
        Err(_) => return HttpResponse::InternalServerError().body("Failed to fetch thread list"),
    };
    println!("Get: Thread Lists");
    
    let urls: Vec<String> = THREAD_REGEX.find_iter(&thread_list_html)
        .map(|capture| capture.as_str().to_string())
        .collect();
    let thread_data = Arc::new(Mutex::new(Vec::<ThreadData>::new()));
    let mut handles = vec![];

    for url in urls {
        println!("Download: {}", url);
        let client = client.clone();
        let query_str = query.query.clone();
        let thread_data = Arc::clone(&thread_data);

        let handle = tokio::spawn(async move {
            if let Ok(resp) = client.get(&url).header(USER_AGENT, USERAGENT).send().await {
                if let Ok(thread_html) = resp.text().await {
                    if let Some(captures) = TITLE_REGEX.captures(&thread_html) {
                        let title = captures[1].to_string();
                        if title.contains(&query_str) {
                            let mut thrs = thread_data.lock().await;
			    println!("Get: {}", title);
                            thrs.push(ThreadData { title, thrurl: url });
                        }
                    }
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    let thrs = thread_data.lock().await;
    let mut ctx = Context::new();
    ctx.insert("URLs", &*thrs);
    let html = tera.render("scan.html", &ctx).unwrap_or_else(|_| "ERROR".into());

    HttpResponse::Ok().body(html)
}

////////////////////////////////////////////////
// START                                      //
////////////////////////////////////////////////


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("V-COM");
    println!("== INFO ========================");
    println!("BUILD-ID:   {}", env!("BUILD_ID"));
    println!("URL:        http://127.0.0.1:8080");
    println!("== LOGS ========================");
    
    let tera = Tera::new("./HTML/*").expect("Tera Init Failure");
    let client = Client::new();
    
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(tera.clone()))
            .app_data(web::Data::new(client.clone()))
            .route("/", web::get().to(index))
            .route("/scan", web::get().to(scan))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
