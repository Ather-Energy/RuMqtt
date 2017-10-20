mod state;
mod connection;

use std::thread;
use std::sync::Arc;
use std::result::Result;
use std::sync::mpsc as stdmpsc;
use std::mem;

use futures::sync::mpsc::{self, Sender};
use futures::{Future, Sink};
use mqtt3::*;

use MqttOptions;
use packet;

use error::Error;
pub use self::connection::Request;

pub struct MqttClient {
    nw_request_tx: Option<Sender<Request>>,
}

impl MqttClient {
    /// Connects to the broker and starts an event loop in a new thread.
    /// Returns 'Request' and handles reqests from it.
    /// Also handles network events, reconnections and retransmissions.
    pub fn start(opts: MqttOptions) -> (Self, stdmpsc::Receiver<Packet>) {
        let (commands_tx, commands_rx) = mpsc::channel(10);
        // used to receive notifications back from network thread
        let (notifier_tx, notifier_rx) = stdmpsc::sync_channel(30);
        let nw_commands_tx = commands_tx.clone();

        // This thread handles network reads (coz they are blocking) and
        // and sends them to event loop thread to handle mqtt state.
        thread::spawn( move || {
                connection::start(opts, nw_commands_tx, commands_rx, notifier_tx);
                error!("Network Thread Stopped !!!!!!!!!");
            }
        );

        let client = MqttClient { nw_request_tx: Some(commands_tx)};
        (client, notifier_rx)
    }

    pub fn publish<S: Into<String>>(&mut self, topic: S, qos: QoS, payload: Vec<u8>) -> Result<(), Error>{
        let payload = Arc::new(payload);

        // NOTE: Don't clone 'tx' as it doubles the queue size for every clone
        let mut nw_request_tx = mem::replace(&mut self.nw_request_tx, None).unwrap();
        
        let publish = packet::gen_publish_packet(topic.into(), qos, None, false, false, payload);
        nw_request_tx = nw_request_tx.send(Request::Publish(publish)).wait()?;

        let _ = mem::replace(&mut self.nw_request_tx, Some(nw_request_tx));
        Ok(())
    }

    // TODO: Add userdata publish

    pub fn subscribe<S: Into<String>>(&mut self, topics: Vec<(S, QoS)>) -> Result<(), Error>{
        if topics.len() == 0 {
            error!("It is invaild to send a subscribe message with zero topics");
            return Err(Error::ZeroSubscriptions);
        }

        let sub_topics: Vec<_> = topics.into_iter().map(
            |t| SubscribeTopic{topic_path: t.0.into(), qos: t.1}
        ).collect();

        // NOTE: Don't clone 'tx' as it doubles the queue size for every clone
        let mut nw_request_tx = mem::replace(&mut self.nw_request_tx, None).unwrap();
        nw_request_tx = nw_request_tx.send(Request::Subscribe(sub_topics)).wait()?;
        let _ = mem::replace(&mut self.nw_request_tx, Some(nw_request_tx));
        Ok(())
    }
}
