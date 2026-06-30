use elan_common::proto::coordinator::{
    coordinator_service_client::CoordinatorServiceClient, HeartbeatRequest,
};
use std::time::Duration;
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use tracing::{error, info, warn};

pub async fn run(coordinator_id: String, mut client: CoordinatorServiceClient<Channel>) {
    let interval = Duration::from_secs(15);

    loop {
        if let Err(e) = heartbeat_loop(&coordinator_id, &mut client).await {
            error!(error = %e, "heartbeat loop failed, retrying in 30s");
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }
}

async fn heartbeat_loop(
    coordinator_id: &str,
    client: &mut CoordinatorServiceClient<Channel>,
) -> anyhow::Result<()> {
    let coordinator_id = coordinator_id.to_string();

    let request_stream = async_stream::stream! {
        loop {
            yield HeartbeatRequest {
                coordinator_id: coordinator_id.clone(),
                sent_at: Some(prost_types::Timestamp {
                    seconds: chrono::Utc::now().timestamp(),
                    nanos: 0,
                }),
            };
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        }
    };

    let mut response_stream = client
        .heartbeat(tonic::Request::new(request_stream))
        .await?
        .into_inner();

    while let Some(response) = response_stream.next().await {
        match response {
            Ok(resp) if resp.alive => {}
            Ok(_) => warn!("central reports coordinator not alive"),
            Err(e) => {
                error!(error = %e, "heartbeat stream error");
                return Err(anyhow::anyhow!(e));
            }
        }
    }

    Ok(())
}
