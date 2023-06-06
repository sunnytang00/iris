use clap::Parser;
use iris_lib::{
    connect::{ConnectionError, ConnectionManager, ConnectionWrite},
    helpers::{
        join_channel, part_channel, private_msg_channel, private_msg_user, quit_server,
        write_to_conn,
    },
    types::{
        Channel, ErrorType, Message, Nick, ParsedMessage, Reply, Target, UnparsedMessage,
        WelcomeReply, SERVER_NAME,
    },
};
use simple_logger::SimpleLogger;
use std::sync::{Arc, Mutex};
use std::thread;
use std::{collections::HashMap, net::IpAddr};

#[derive(Parser)]
struct Arguments {
    #[clap(default_value = "127.0.0.1")]
    ip_address: IpAddr,

    #[clap(default_value = "6991")]
    port: u16,
}

fn main() {
    // Initalise logging
    SimpleLogger::new().init().unwrap();
    let arguments = Arguments::parse();
    println!(
        "Launching {} at {}:{}",
        SERVER_NAME, arguments.ip_address, arguments.port
    );
    // Hashmap for storing conn_writes of users
    let user_map: Arc<Mutex<HashMap<Nick, ConnectionWrite>>> = Arc::new(Mutex::new(HashMap::new()));
    // Hashmap for storing channels and their users
    let channels: Arc<Mutex<HashMap<Channel, Vec<Nick>>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut connection_manager = ConnectionManager::launch(arguments.ip_address, arguments.port);
    loop {
        // This function call will block until a new client connects!
        let (mut conn_read, mut conn_write) = connection_manager.accept_new_connection();
        let user_map_clone = user_map.clone();
        let channels_clone = channels.clone();
        // Spawn a thread for each client that connects
        thread::spawn(move || {
            println!("New connection from {}", conn_read.id());
            let mut nicked = false;
            let mut nickname = Nick("unregistered user".to_string());

            // First loop only accepts nick/user command - ignores all else
            loop {
                println!("Waiting for message...");
                let message = match conn_read.read_message() {
                    Ok(message) => message,
                    Err(ConnectionError::ConnectionLost | ConnectionError::ConnectionClosed) => {
                        println!("Lost connection.");
                        break;
                    }
                    Err(_) => {
                        println!("Invalid message received... ignoring message.");
                        continue;
                    }
                };

                log::info!("Received from {}: {}", nickname, message);

                match ParsedMessage::try_from(UnparsedMessage {
                    message: &message,
                    sender_nick: Nick("empty".to_string()),
                }) {
                    Ok(parsed) => match parsed.message {
                        Message::Nick(nick_msg) => {
                            nickname = nick_msg.nick;
                            let user_map_mutex = user_map_clone.lock().unwrap();

                            if user_map_mutex.contains_key(&nickname) {
                                let _ = conn_write
                                    .write_message(&format!("{}\r\n", ErrorType::NickCollision));
                                log::warn!(
                                    "Sent to {}: {}",
                                    conn_read.id(),
                                    ErrorType::NickCollision
                                );
                            } else {
                                nicked = true;
                            }
                        }

                        Message::User(user_msg) => {
                            if nicked {
                                let username = user_msg.real_name;
                                let reply = WelcomeReply {
                                    target_nick: Nick(nickname.to_string()),
                                    message: format!("Welcome to this server, {}!", username),
                                };
                                write_to_conn(
                                    &nickname,
                                    &mut conn_write,
                                    format!("{}", Reply::Welcome(reply)),
                                );

                                let mut user_map_mutex = user_map_clone.lock().unwrap();
                                user_map_mutex.insert(nickname.clone(), conn_write);
                                // Break out of loop once valid nick/user is entered
                                break;
                            }
                        }

                        _ => {}
                    },
                    Err(err) => {
                        let _ = conn_write.write_message(&format!("{}\r\n", err));
                        log::error!("Sent to {}: {}", nickname, err);
                    }
                };
            }

            // This loop handles all the commands once user has nicked/usered
            loop {
                println!("Waiting for message...");
                let message = match conn_read.read_message() {
                    Ok(message) => message,
                    Err(ConnectionError::ConnectionLost | ConnectionError::ConnectionClosed) => {
                        println!("Lost connection.");
                        break;
                    }
                    Err(_) => {
                        println!("Invalid message received... ignoring message.");
                        continue;
                    }
                };

                log::info!("Received from {}: {}", nickname, message);

                match ParsedMessage::try_from(UnparsedMessage {
                    message: &message,
                    sender_nick: Nick("empty".to_string()),
                }) {
                    Ok(parsed) => match parsed.message {
                        Message::PrivMsg(priv_msg) => match priv_msg.target {
                            Target::Channel(channel) => {
                                let channels_mutex = channels_clone.lock().unwrap();
                                private_msg_channel(
                                    channels_mutex,
                                    user_map_clone.clone(),
                                    channel,
                                    priv_msg.message.clone(),
                                    nickname.clone(),
                                );
                            }
                            Target::User(user) => {
                                let user_map_mutex = user_map_clone.lock().unwrap();
                                private_msg_user(
                                    user_map_mutex,
                                    &nickname,
                                    user,
                                    priv_msg.message.clone(),
                                );
                            }
                        },
                        Message::Ping(ping_msg) => {
                            let mut user_map_mutex = user_map_clone.lock().unwrap();
                            let c_write = user_map_mutex.get_mut(&nickname).unwrap();
                            write_to_conn(
                                &nickname,
                                c_write,
                                format!("{}", Reply::Pong(ping_msg.clone())),
                            );
                            log::info!("Sent to {}: PONG {}", nickname, ping_msg);
                        }
                        Message::Join(join_msg) => {
                            let channels_mutex = channels_clone.lock().unwrap();
                            join_channel(
                                channels_mutex,
                                user_map_clone.clone(),
                                &nickname,
                                join_msg,
                            );
                        }
                        Message::Part(part_msg) => {
                            // Obtain conn write
                            let channels_mutex = channels_clone.lock().unwrap();
                            part_channel(
                                channels_mutex,
                                user_map_clone.clone(),
                                part_msg,
                                &nickname,
                            );
                        }
                        Message::Quit(quit_msg) => {
                            //save quit msg
                            let message = match quit_msg.message {
                                Some(msg) => msg,
                                None => nickname.to_string(),
                            };
                            //go through list of channels and check if user was in it, if so send msg to everyone
                            let channels_mutex = channels_clone.lock().unwrap();
                            quit_server(channels_mutex, user_map_clone, &nickname, message);
                            break;
                        }
                        _ => {}
                    },
                    Err(err) => {
                        let mut user_map_mutex = user_map_clone.lock().unwrap();
                        let c_write = user_map_mutex.get_mut(&nickname).unwrap();
                        let _ = c_write.write_message(&format!("{}\r\n", err));
                        log::error!("Sent to {}: {}", nickname, err);
                    }
                };
            }
        });
    }
}
