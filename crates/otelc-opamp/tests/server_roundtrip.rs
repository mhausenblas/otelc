//! End-to-end test of the embedded OpAMP server against a WebSocket client.

use futures_util::{SinkExt, StreamExt};
use otelc_opamp::{framing, pb, InstanceUid, OpampEvent, OpampServer, ServerConfig};
use tokio_tungstenite::tungstenite::Message as Ws;

async fn next_s2a(
    read: &mut (impl StreamExt<Item = Result<Ws, tokio_tungstenite::tungstenite::Error>> + Unpin),
) -> pb::ServerToAgent {
    loop {
        match read.next().await.expect("stream closed").expect("ws error") {
            Ws::Binary(data) => return framing::decode(&data).expect("decode ServerToAgent"),
            _ => continue,
        }
    }
}

#[tokio::test]
async fn agent_connects_gets_offer_and_remote_config() {
    let (handle, mut events) = OpampServer::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        otlp_offer: Some("http://127.0.0.1:4317".to_string()),
    })
    .await
    .expect("server starts");

    let url = format!("ws://{}/v1/opamp", handle.local_addr());
    let (ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("client connects");
    let (mut write, mut read) = ws.split();

    let uid = InstanceUid::new_v7();
    let first = pb::AgentToServer {
        instance_uid: uid.to_vec(),
        sequence_num: 1,
        capabilities: pb::AgentCapabilities::ReportsStatus as u64
            | pb::AgentCapabilities::AcceptsRemoteConfig as u64
            | pb::AgentCapabilities::ReportsOwnMetrics as u64,
        ..Default::default()
    };
    write
        .send(Ws::Binary(framing::encode(&first).into()))
        .await
        .expect("send first message");

    // The server surfaces the message as an event.
    match events.recv().await.expect("event") {
        OpampEvent::Message { uid: got, .. } => assert_eq!(got, uid),
        other => panic!("unexpected event: {other:?}"),
    }

    // The first reply carries an own-telemetry connection-settings offer.
    let reply = next_s2a(&mut read).await;
    assert_eq!(reply.instance_uid, uid.to_vec());
    assert!(
        reply.connection_settings.is_some(),
        "agent reporting own metrics should receive an OwnTelemetry offer"
    );

    // A pushed remote config reaches the agent over the same connection.
    let hash = handle
        .offer_remote_config(&uid, "service: {}")
        .expect("offer remote config");
    let offer = next_s2a(&mut read).await;
    let remote = offer.remote_config.expect("remote config present");
    assert_eq!(remote.config_hash, hash);
    let body = &remote.config.unwrap().config_map[""].body;
    assert_eq!(body, b"service: {}");
}

#[tokio::test]
async fn restart_command_is_delivered() {
    let (handle, mut events) = OpampServer::start(ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        otlp_offer: None,
    })
    .await
    .expect("server starts");

    let url = format!("ws://{}/v1/opamp", handle.local_addr());
    let (ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("client connects");
    let (mut write, mut read) = ws.split();

    let uid = InstanceUid::new_v7();
    write
        .send(Ws::Binary(
            framing::encode(&pb::AgentToServer {
                instance_uid: uid.to_vec(),
                sequence_num: 1,
                capabilities: pb::AgentCapabilities::ReportsStatus as u64,
                ..Default::default()
            })
            .into(),
        ))
        .await
        .expect("send first message");
    let _ = events.recv().await.expect("event");
    let _first_reply = next_s2a(&mut read).await;

    handle.send_restart(&uid).expect("send restart");
    let command = next_s2a(&mut read).await;
    let cmd = command.command.expect("restart command present");
    assert_eq!(cmd.r#type, pb::CommandType::Restart as i32);
}
