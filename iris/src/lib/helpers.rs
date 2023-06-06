use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::{
    connect::ConnectionWrite,
    types::{
        Channel, ErrorType, JoinMsg, JoinReply, Nick, PartMsg, PartReply, PrivMsg, PrivReply,
        QuitMsg, QuitReply, Reply, Target,
    },
};

pub fn write_to_conn(target_nick: &Nick, target_conn: &mut ConnectionWrite, conn_message: String) {
    match target_conn.write_message(&conn_message) {
        Ok(_) => {
            log::info!("Sent to {}: {}", target_nick, conn_message);
        }
        Err(_) => {
            log::error!("Unable to send message to client.");
        }
    };
}

pub fn private_msg_channel(
    channel_mutex: MutexGuard<HashMap<Channel, Vec<Nick>>>,
    user_map_clone: Arc<Mutex<HashMap<Nick, ConnectionWrite>>>,
    channel: Channel,
    priv_msg: String,
    nickname: Nick,
) {
    match channel_mutex.get(&channel) {
        Some(list) => {
            list.iter().for_each(|nick| {
                let mut user_map_mutex = user_map_clone.lock().unwrap();
                let c_write = user_map_mutex.get_mut(nick).unwrap();
                write_to_conn(
                    nick,
                    c_write,
                    format!(
                        "{}",
                        Reply::PrivMsg(PrivReply {
                            message: PrivMsg {
                                target: Target::Channel(Channel((channel).to_string())),
                                message: priv_msg.clone(),
                            },
                            sender_nick: nickname.clone()
                        })
                    ),
                );
            });
        }
        None => {
            let mut user_map_mutex = user_map_clone.lock().unwrap();
            let c_write = user_map_mutex.get_mut(&nickname).unwrap();
            write_to_conn(
                &nickname,
                c_write,
                format!("{}\r\n", ErrorType::NoSuchChannel),
            );
        }
    }
}

pub fn private_msg_user(
    mut user_map_mutex: MutexGuard<HashMap<Nick, ConnectionWrite>>,
    nickname: &Nick,
    user: Nick,
    priv_msg: String,
) {
    if user_map_mutex.contains_key(&user) {
        let c_write = user_map_mutex.get_mut(&user).unwrap();
        write_to_conn(
            &user,
            c_write,
            format!(
                "{}",
                Reply::PrivMsg(PrivReply {
                    message: PrivMsg {
                        target: Target::User(user.clone()),
                        message: priv_msg,
                    },
                    sender_nick: nickname.clone()
                })
            ),
        );
    } else {
        let c_write = user_map_mutex.get_mut(nickname).unwrap();
        write_to_conn(&user, c_write, format!("{}\r\n", ErrorType::NoSuchNick));
    }
}

pub fn join_channel(
    mut channel_mutex: MutexGuard<HashMap<Channel, Vec<Nick>>>,
    user_map_clone: Arc<Mutex<HashMap<Nick, ConnectionWrite>>>,
    nickname: &Nick,
    join_msg: JoinMsg,
) {
    match channel_mutex.get_mut(&join_msg.channel) {
        Some(list) => {
            if !list.contains(nickname) {
                list.push(nickname.clone());
                list.iter().for_each(|nick| {
                    let mut user_map_mutex = user_map_clone.lock().unwrap();
                    let c_write = user_map_mutex.get_mut(nick).unwrap();
                    write_to_conn(
                        nick,
                        c_write,
                        format!(
                            "{}",
                            Reply::Join(JoinReply {
                                message: JoinMsg {
                                    channel: Channel(join_msg.channel.to_string())
                                },
                                sender_nick: nickname.clone()
                            })
                        ),
                    );
                });
            }
        }
        None => {
            let mut user_map_mutex = user_map_clone.lock().unwrap();
            let c_write = user_map_mutex.get_mut(nickname).unwrap();
            write_to_conn(
                nickname,
                c_write,
                format!(
                    "{}",
                    Reply::Join(JoinReply {
                        message: JoinMsg {
                            channel: Channel(join_msg.channel.to_string())
                        },
                        sender_nick: nickname.clone()
                    })
                ),
            );
            channel_mutex.insert(join_msg.channel, vec![nickname.clone()]);
        }
    }
}

pub fn part_channel(
    mut channel_mutex: MutexGuard<HashMap<Channel, Vec<Nick>>>,
    user_map_clone: Arc<Mutex<HashMap<Nick, ConnectionWrite>>>,
    part_msg: PartMsg,
    nickname: &Nick,
) {
    match channel_mutex.get_mut(&part_msg.channel) {
        Some(list) => {
            if list.contains(nickname) {
                list.iter().for_each(|nick| {
                    let mut user_map_mutex = user_map_clone.lock().unwrap();
                    let c_write = user_map_mutex.get_mut(nick).unwrap();
                    write_to_conn(
                        nick,
                        c_write,
                        format!(
                            "{}",
                            Reply::Part(PartReply {
                                message: PartMsg {
                                    channel: Channel(part_msg.channel.to_string())
                                },
                                sender_nick: nickname.clone()
                            })
                        ),
                    );
                });
                list.retain(|x| x != nickname);
            }
        }
        None => {
            //return no such channel error
            let mut user_map_mutex = user_map_clone.lock().unwrap();
            let c_write = user_map_mutex.get_mut(nickname).unwrap();
            let _ = c_write.write_message(format!("{}\r\n", ErrorType::NoSuchChannel).as_str());
        }
    }
}

pub fn quit_server(
    mut channel_mutex: MutexGuard<HashMap<Channel, Vec<Nick>>>,
    user_map_clone: Arc<Mutex<HashMap<Nick, ConnectionWrite>>>,
    nickname: &Nick,
    message: String,
) {
    for (_channel, channel_users) in channel_mutex.iter_mut() {
        if channel_users.contains(nickname) {
            channel_users.retain(|user| user != nickname);
            channel_users.iter().for_each(|nick| {
                let mut user_map_mutex = user_map_clone.lock().unwrap();
                let c_write = user_map_mutex.get_mut(nick).unwrap();
                write_to_conn(
                    nick,
                    c_write,
                    format!(
                        "{}",
                        Reply::Quit(QuitReply {
                            message: QuitMsg {
                                message: Some(message.clone())
                            },
                            sender_nick: nickname.clone()
                        })
                    ),
                );
            });
        }
    }
    let mut user_map_mutex = user_map_clone.lock().unwrap();
    user_map_mutex.remove(nickname);
}
