use anyhow::Result;
use wiremock::{MockServer, Mock, ResponseTemplate, MockGuard};
use wiremock::matchers::{method, path};

pub struct TelemetryCapture {
    pub telemetry_mock: MockGuard,
}

impl TelemetryCapture {
    pub async fn new() -> Result<Self> {
        // Create a wiremock server
        let mock_server = MockServer::start().await;
        let server_url = mock_server.uri();

        // Set up mock for /api/send endpoint
        let telemetry_mock = Mock::given(method("POST"))
            .and(path("/api/send"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({}))
                .append_header("content-type", "application/json"))
            .mount_as_scoped(&mock_server)
            .await;

        let endpoint = server_url.clone() + "/api/send";
        std::env::set_var("POP_TELEMETRY_ENDPOINT", endpoint);
        std::env::remove_var("DO_NOT_TRACK");
        std::env::remove_var("CI");


        Ok(Self {
            telemetry_mock,
        })

    }

    // pub async fn wait_for_calls(&self) -> Result<()> {
    //     // Wait for telemetry calls to complete
    //     tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    //     Ok(())
    // }

    async fn parse_payload_from_request(&self, request_index: Option<usize>) -> Result<(String, String)> {
        // Get the received requests from wiremock
        let requests = self.telemetry_mock.received_requests().await;
        
        let request = match request_index {
            Some(index) => {
                assert!(index < requests.len(), "Request index {} out of range (got {} requests)", index, requests.len());
                &requests[index]
            }
            None => {
                assert!(!requests.is_empty(), "No requests received");
                requests.last().unwrap()
            }
        };
        
        let body = String::from_utf8(request.body.clone()).unwrap_or_default();
        
        // Parse the JSON body
        let payload: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("Failed to parse request body as JSON: {}", e))?;
        
        let actual_name = payload["payload"]["name"].as_str().unwrap_or("").to_string();
        let actual_data = payload["payload"]["data"].as_str().unwrap_or("").to_string();
        
        Ok((actual_name, actual_data))
    }

    pub async fn assert_latest_payload_structure(&self, expected_name: &str, expected_data: &str) -> Result<()> {
        let (actual_name, actual_data) = self.parse_payload_from_request(None).await?;
        
        assert_eq!(actual_name, expected_name);
        assert_eq!(actual_data, expected_data);
        
        Ok(())
    }
}

pub fn cleanup_telemetry_env() {
    std::env::remove_var("POP_TELEMETRY_ENDPOINT");
}
