extern crate rustc_serialize;
extern crate iron;

use iron::prelude::*;
use iron::status;
use rustc_serialize::{json, Encodable, Encoder, Decoder};
use std::process::Command;
use std::collections::HashMap;
use std::net::{SocketAddrV4, Ipv4Addr};
use std::str;
use std::io::Read;
use std::sync::{Arc, Mutex};

#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
struct Comment {
    username: String,
    timestamp: i64,
    body: String
}

#[derive(PartialEq, Eq, Debug, RustcDecodable)]
struct PostComment {
    page: String,
    comment: Comment
}

#[derive(Debug)]
struct CommentSystem {
    threads: Box<HashMap<String, Vec<Comment>>>
}

impl CommentSystem {
    fn post_comment(&mut self, req: &mut Request) -> IronResult<Response> {
        let mut content = String::new();
        if let Ok(_) = req.body.read_to_string(&mut content) {
            let new_comment = json::decode::<PostComment>(&content);
            match new_comment {
                Ok(post_comment) => {
                    let posted = if let Some(thread) = self.threads.get_mut(&post_comment.page) {
                        thread.push(post_comment.comment.clone());
                        true
                    } else { false };
                    if !posted {
                        self.threads.insert(post_comment.page, vec!(post_comment.comment));
                    }
                    Ok(Response::with((status::Ok, "Comment accepted!\n")))
                },
                Err(_) => Ok(Response::with((status::BadRequest, "Bad format.\n")))
            }
        } else {
            Ok(Response::with((status::BadRequest, "Failed to read body.\n")))
        }
    }

    fn get_comments(&self, req: &Request) -> IronResult<Response> {
      match req.url.query {
        Some(ref page_request) => {
          println!("{}", page_request);
          match self.threads.get(page_request) {
            Some(thread) =>
              Ok(Response::with((status::Ok, json::encode(thread).unwrap() + "\n"))),
            None =>
              Ok(Response::with((status::NotFound, "Page name not found\n")))
          }
        }
        None => 
          Ok(Response::with((status::BadRequest, "No page specified.\n")))
      }
    }
}

struct Server {
    comment_system: Arc<Mutex<CommentSystem>>
}

impl iron::Handler for Server {
    fn handle(&self, req: &mut Request) -> IronResult<Response> {
        match req.method {
            iron::method::Get =>
              self.comment_system.lock().unwrap().get_comments(req),
            iron::method::Post =>
                self.comment_system.lock().unwrap().post_comment(req),
            _ => Ok(Response::with((status::MethodNotAllowed, "Inaccessible.\n")))
        }
    }
}
    
fn main() {
    let comment_system: CommentSystem = CommentSystem{threads: Box::new(HashMap::new()) };

    let host_port = 8080;
    let hostname_cmd = 
        Command::new("hostname")
            .arg("-I")
            .output();
    let host_addr: SocketAddrV4 = match hostname_cmd {
        Result::Ok(res) => {
            let addr = str::from_utf8(res.stdout.as_slice())
                .map_err(|err| err.to_string())
                .and_then(|ip_str| ip_str
                                    .trim()
                                    .parse::<Ipv4Addr>()
                                    .map_err(|err| err.to_string()))
                .map(|ip| SocketAddrV4::new(ip, host_port));

            match addr {
                Ok(addr) => addr,
                Err(_) => {
                    let ip = Ipv4Addr::new(127, 0, 0, 1);
                    SocketAddrV4::new(ip, host_port)
                }
            }
        },
        Result::Err(_) => {
            let ip = Ipv4Addr::new(127, 0, 0, 1);
            SocketAddrV4::new(ip, host_port)
        }
    };
 
    let server = Server{comment_system: Arc::new(Mutex::new(comment_system))};
    println!("Server listening on {}", host_addr);
    Iron::new(server).http(host_addr).unwrap();
}

