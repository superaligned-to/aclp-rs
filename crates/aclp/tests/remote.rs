//! Named public hosts, client identity, consent, refreshing credentials and discovery.

mod common;

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use aclp::events::{
    AuthMethod, ClientInfo, ClientUnregisteredParams, DiscoveryAuth, DiscoveryDocument,
    InputSubmittedParams, SessionStartedParams, SoftwareRef, ToolFinishedParams,
};
use aclp::{
    CallContext, Callbacks, Client, ClientError, ClientOptions, ConfigError, DiscoverOptions,
    Discovery, Server, ServerOptions, discover,
};
use common::{ContextReply, context_schema, input, post, start, tool_message};

fn identity() -> ClientInfo {
    ClientInfo {
        name: "aclp-hooks".to_owned(),
        version: Some("0.1.0".to_owned()),
        harness: Some(SoftwareRef {
            name: "claude-code".to_owned(),
            version: Some("2.1.4".to_owned()),
        }),
    }
}

fn session() -> SessionStartedParams {
    SessionStartedParams {
        session_id: "session-1".to_owned(),
        run_id: None,
        agent_id: None,
        reason: None,
        parent_session_id: None,
    }
}

#[tokio::test]
async fn named_public_hosts_are_served_only_with_authorization() {
    let hosts = ServerOptions::new().hosts(["aclp.example.com"]);

    assert_eq!(
        Server::new(Callbacks::new(), hosts).err(),
        Some(ConfigError::Hosts)
    );

    let ported = ServerOptions::new()
        .hosts(["aclp.example.com:443"])
        .authorization("Bearer t");

    assert_eq!(
        Server::new(Callbacks::new(), ported).err(),
        Some(ConfigError::Hosts)
    );

    let calls = Arc::new(AtomicUsize::new(0));

    let counter = Arc::clone(&calls);

    let callbacks = Callbacks::new().notification(move |_: ToolFinishedParams| {
        counter.fetch_add(1, Ordering::SeqCst);

        async { Ok::<_, Infallible>(()) }
    });

    let opts = ServerOptions::new()
        .hosts(["aclp.example.com"])
        .authorization("Bearer t");

    let server = start(callbacks, opts).await;

    let body = tool_message().to_string();

    let auth = ("Authorization", "Bearer t");

    let status = |host: &'static str, extra: Vec<(&'static str, &'static str)>| {
        let body = body.clone();

        async move {
            let mut headers = vec![("Host", host)];

            headers.extend(extra);
            post(server.port, &body, &headers).await.status
        }
    };

    assert_eq!(status("aclp.example.com", vec![auth]).await, 202);
    assert_eq!(status("aclp.example.com:443", vec![auth]).await, 202);
    assert_eq!(status("aclp.example.com", vec![]).await, 401);
    assert_eq!(status("other.example.com", vec![auth]).await, 403);
    assert_eq!(
        status(
            "aclp.example.com",
            vec![auth, ("Origin", "https://aclp.example.com")]
        )
        .await,
        403
    );
    server.stop().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn callbacks_see_the_client_identity_and_clients_send_only_consented_events() {
    let seen = Arc::new(Mutex::new(Vec::new()));

    let sink = Arc::clone(&seen);

    let callbacks = Callbacks::new().request_with(
        &context_schema(),
        move |_: InputSubmittedParams, ctx: CallContext| {
            sink.lock().expect("lock").push(ctx.client);

            async {
                Ok::<_, Infallible>(ContextReply {
                    context: Vec::new(),
                })
            }
        },
    );

    let server = start(
        callbacks,
        ServerOptions::new().authorization("Bearer rotated-1"),
    )
    .await;

    let refreshed = Arc::new(AtomicUsize::new(0));

    let counter = Arc::clone(&refreshed);

    let opts = ClientOptions::new(server.url())
        .client(identity())
        .events(["input.submitted"])
        .on_error(|_| {})
        .authorization_with(move || {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;

            async move { format!("Bearer rotated-{n}") }
        });

    let client = Client::new(opts).expect("client config");

    client
        .request(&input(), &context_schema())
        .await
        .expect("consented request");
    assert_eq!(
        seen.lock().expect("lock").as_slice(),
        [Some("aclp-hooks/0.1.0 claude-code/2.1.4".to_owned())]
    );
    assert!(matches!(
        client.notify(&session()),
        Err(ClientError::NotConsented {
            method: "session.started"
        })
    ));
    client
        .notify(&ClientUnregisteredParams { client: identity() })
        .expect("client methods are always allowed");
    assert!(
        client.flush().await.is_err(),
        "rotated-2 is refused and the method is unserved"
    );
    assert_eq!(refreshed.load(Ordering::SeqCst), 2);

    let blank = ClientInfo {
        name: String::new(),
        version: None,
        harness: None,
    };

    assert_eq!(
        Client::new(ClientOptions::new(server.url()).client(blank)).err(),
        Some(ConfigError::Identity)
    );
    server.stop().await;
}

#[tokio::test]
async fn discovery_describes_registered_callbacks_and_rejects_invalid_documents() {
    let callbacks = Callbacks::new()
        .request(&context_schema(), |_: InputSubmittedParams| async {
            Ok::<_, Infallible>(ContextReply {
                context: Vec::new(),
            })
        })
        .notification(|_: ToolFinishedParams| async { Ok::<_, Infallible>(()) });

    let opts = ServerOptions::new()
        .authorization("Bearer t")
        .discovery(Discovery::new("https://aclp.example.com/events").name("Example"));

    let server = start(callbacks, opts).await;

    let doc = discover(
        &format!("http://127.0.0.1:{}/anything", server.port),
        DiscoverOptions::new(),
    )
    .await
    .expect("discovery document");

    assert_eq!(
        doc,
        DiscoveryDocument {
            name: Some("Example".to_owned()),
            endpoint: "https://aclp.example.com/events".to_owned(),
            auth: DiscoveryAuth {
                methods: vec![AuthMethod::Token],
                oauth: None,
            },
            events: vec!["input.submitted".to_owned(), "tool.finished".to_owned()],
            requests: vec!["input.submitted".to_owned()],
        }
    );
    server.stop().await;

    let plain = start(Callbacks::new(), ServerOptions::new()).await;

    let missing = discover(&plain.url(), DiscoverOptions::new()).await;

    assert!(
        matches!(missing, Err(ClientError::Status(404))),
        "{missing:?}"
    );
    plain.stop().await;

    let oauth = DiscoveryAuth {
        methods: vec![AuthMethod::Oauth],
        oauth: None,
    };

    let invalid = ServerOptions::new()
        .discovery(Discovery::new("https://aclp.example.com/events").auth(oauth));

    assert_eq!(
        Server::new(Callbacks::new(), invalid).err(),
        Some(ConfigError::Discovery)
    );
}
