extern crate rustc_serialize;
extern crate iron;
extern crate bincode;

use iron::prelude::*;
use iron::status;

use rustc_serialize::{json, Encodable, Encoder, Decoder};

use bincode::SizeLimit;
use bincode::rustc_serialize::{encode, decode};

use std::env;
use std::str;
use std::io::{BufWriter, BufReader, Read};
use std::process::{Command, exit};
use std::collections::HashMap;
use std::net::{SocketAddrV4, Ipv4Addr};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::fs::{File, OpenOptions};
use std::path::Path;

#[derive(PartialEq, Eq, Clone, Debug, RustcEncodable, RustcDecodable)]
struct Comment {
    username: String,
    timestamp: i64,
    body: String
}

#[derive(PartialEq, Eq, Clone, Debug, RustcDecodable, RustcEncodable)]
struct PostComment {
    page: String,
    comment: Comment
}

struct Storage {
    recv: mpsc::Receiver<PostComment>,
    storage_location: File
}

impl Storage {
    fn new(rx: mpsc::Receiver<PostComment>, path: &Path) -> Storage {
      let fp = path.join("comments.fdp");
      let f = OpenOptions::new()
                .read(true)
                .append(true)
                .create(true)
                .open(fp);

      Storage{ recv: rx, storage_location: f.unwrap() }
    }

    fn save(&mut self, pcomment: PostComment) {
      let mut writer = BufWriter::new(&self.storage_location);
      bincode::rustc_serialize::encode_into(&pcomment, &mut writer, bincode::SizeLimit::Infinite).unwrap();
    }

    fn load(&self) -> Vec<PostComment> {
      let mut reader = BufReader::new(&self.storage_location);
      let mut data: Vec<PostComment> = Vec::new();
      while let Ok(decoded) = bincode::rustc_serialize::decode_from(&mut reader,
                                                                    bincode::SizeLimit::Infinite) {
        data.push(decoded)
      }
      data
    }
}

#[derive(Debug)]
struct CommentSystem {
    threads: Box<HashMap<String, Vec<Comment>>>,
    storage_send: mpsc::Sender<PostComment>
}

impl CommentSystem {
    fn post_comment(&mut self, req: &mut Request) -> IronResult<Response> {
        let mut content = String::new();
        if let Ok(_) = req.body.read_to_string(&mut content) {
            let new_comment = json::decode::<PostComment>(&content);
            match new_comment {
                Ok(post_comment) => {
                  self.insert_comment(&post_comment);
                  self.storage_send.send(post_comment);
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

    fn insert_comment(&mut self, comment: &PostComment) {
      let inserted = if let Some(thread) = self.threads.get_mut(&comment.page) {
        thread.push(comment.comment.clone());
        true
      } else { false };
      if !inserted {
        let inserting = comment.clone();
        self.threads.insert(inserting.page, vec!(inserting.comment));
      }
    }

    fn insert_comments(&mut self, comments: Vec<PostComment>) {
      for pcmt in comments.iter() {
        self.insert_comment(pcmt);
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
    let (tx, rx) = mpsc::channel();
    let mut comment_system: CommentSystem = CommentSystem {
      threads: Box::new(HashMap::new()),
      storage_send: tx
    };

    let backup_path_name = if let Ok(path_name) = env::var("FISCHERI_PATH") {
      println!("Saving backup data in directory {}", path_name);
      path_name
    } else {
      println!("Please set FISCHERI_PATH to a valid directory path for storing back-up data");
      exit(0);
    };

    let storage_path = Path::new(&backup_path_name);

    if !storage_path.exists() {
      println!("Please set FISCHERI_PATH to a valid directory path for storing back-up data");
      exit(0);
    };

    let mut storage = Storage::new(rx, storage_path);

    let comments_on_disk = storage.load();

    comment_system.insert_comments(comments_on_disk);

    thread::spawn(move || {
      while let Ok(data) = storage.recv.recv() {
        println!("{:?}", data);
        storage.save(data);
      }
    });

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

