use crate::modules::{base, Command, Module};
use clap::{Arg, ArgMatches, SubCommand};
use image::Luma;
use qrcode::QrCode;
use std::io::{self, Cursor, Write};

pub fn module<'a, 'b>() -> Module<'a, 'b> {
	Module {
		desc: "QR Code generation".to_string(),
		commands: commands(),
		get_cases: cases::cases,
	}
}

pub fn commands<'a, 'b>() -> Vec<Command<'a, 'b>> {
	vec![Command {
		app: SubCommand::with_name("s2qr")
			.about("Convert string to QR code (PNG)")
			.arg(Arg::with_name("INPUT").required(false).index(1)),
		f: s2qr,
	},
	Command {
		app: SubCommand::with_name("qr2s")
			.about("Convert QR code image to string")
			.arg(Arg::with_name("INPUT").required(false).index(1)), // Kept for compatibility but we read from stdin
		f: qr2s,
	}]
}

fn s2qr(matches: &ArgMatches) -> Result<Vec<String>, String> {
	let input = base::input_string(matches)?;

	let code = QrCode::new(input.as_bytes()).map_err(|e| format!("Failed to generate QR code: {}", e))?;

	let image = code.render::<Luma<u8>>().build();

	let mut buffer = Vec::new();
	let mut cursor = Cursor::new(&mut buffer);
	image
		.write_to(&mut cursor, image::ImageFormat::Png)
		.map_err(|e| format!("Failed to write image: {}", e))?;

	io::stdout()
		.write_all(&buffer)
		.map_err(|e| format!("Failed to write to stdout: {}", e))?;

	Ok(vec![])
}

fn qr2s(_matches: &ArgMatches) -> Result<Vec<String>, String> {
	// Use tokio runtime for async operations
	let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("Failed to create runtime: {}", e))?;
	
	runtime.block_on(async {
		run_qr_scanner().await
	})
}

async fn run_qr_scanner() -> Result<Vec<String>, String> {
	use axum::{
		routing::{get, post},
		Router,
		response::Html,
		Json,
	};
	use std::net::TcpListener;
	use tokio::sync::oneshot;
	
	// Create a channel to receive the QR code result
	let (tx, rx) = oneshot::channel::<String>();
	let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
	
	// HTML page with QR scanner
	let html = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>QR Code Scanner</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            min-height: 100vh;
            margin: 0;
            background: #f0f0f0;
        }
        #video-container {
            position: relative;
            width: 90vw;
            max-width: 500px;
            aspect-ratio: 1;
            background: #000;
            border-radius: 10px;
            overflow: hidden;
        }
        video {
            width: 100%;
            height: 100%;
            object-fit: cover;
        }
        #result {
            margin-top: 20px;
            padding: 15px;
            background: white;
            border-radius: 5px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
            max-width: 500px;
            word-break: break-all;
        }
        .success {
            color: green;
            font-weight: bold;
        }
        h1 {
            color: #333;
        }
    </style>
</head>
<body>
    <h1>QR Code Scanner</h1>
    <div id="video-container">
        <video id="video"></video>
    </div>
    <div id="result">Waiting for QR code...</div>
    
    <script type="module">
        import QrScanner from 'https://cdn.jsdelivr.net/npm/qr-scanner@1.4.2/qr-scanner.min.js';
        
        const video = document.getElementById('video');
        const resultDiv = document.getElementById('result');
        
        const qrScanner = new QrScanner(
            video,
            result => {
                resultDiv.innerHTML = '<span class="success">QR Code detected!</span><br>' + result.data;
                qrScanner.stop();
                
                // Send result to server
                fetch('/result', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({ data: result.data })
                }).then(() => {
                    setTimeout(() => {
                        resultDiv.innerHTML += '<br><br>You can close this window now.';
                    }, 500);
                });
            },
            {
                returnDetailedScanResult: true,
                highlightScanRegion: true,
                highlightCodeOutline: true,
            }
        );
        
        qrScanner.start().catch(err => {
            resultDiv.textContent = 'Error: ' + err;
        });
    </script>
</body>
</html>
"#;
	
	let html_clone = html.to_string();
	let tx_clone = tx.clone();
	
	// Create the router
	let app = Router::new()
		.route("/", get(move || async move { Html(html_clone) }))
		.route("/result", post(move |Json(payload): Json<serde_json::Value>| async move {
			if let Some(data) = payload.get("data").and_then(|v| v.as_str()) {
				if let Some(sender) = tx_clone.lock().unwrap().take() {
					let _ = sender.send(data.to_string());
				}
			}
			"OK"
		}));
	
	// Bind to a random port
	let listener = TcpListener::bind("127.0.0.1:0")
		.map_err(|e| format!("Failed to bind to port: {}", e))?;
	let addr = listener.local_addr()
		.map_err(|e| format!("Failed to get local address: {}", e))?;
	
	let url = format!("http://{}", addr);
	eprintln!("QR Scanner running at: {}", url);
	
	// Open browser
	if let Err(e) = open_browser(&url) {
		eprintln!("Failed to open browser: {}. Please open {} manually.", e, url);
	}
	
	// Convert std TcpListener to tokio
	listener.set_nonblocking(true)
		.map_err(|e| format!("Failed to set non-blocking: {}", e))?;
	let listener = tokio::net::TcpListener::from_std(listener)
		.map_err(|e| format!("Failed to convert listener: {}", e))?;
	
	// Spawn server in background
	let server = axum::serve(listener, app);
	let server_handle = tokio::spawn(async move {
		server.await
	});
	
	// Wait for result
	let result = rx.await
		.map_err(|_| "Failed to receive QR code result".to_string())?;
	
	// Abort server
	server_handle.abort();
	
	Ok(vec![result])
}

fn open_browser(url: &str) -> Result<(), String> {
	#[cfg(target_os = "macos")]
	{
		std::process::Command::new("open")
			.arg(url)
			.spawn()
			.map_err(|e| e.to_string())?;
	}
	
	#[cfg(target_os = "linux")]
	{
		std::process::Command::new("xdg-open")
			.arg(url)
			.spawn()
			.map_err(|e| e.to_string())?;
	}
	
	#[cfg(target_os = "windows")]
	{
		std::process::Command::new("cmd")
			.args(&["/C", "start", url])
			.spawn()
			.map_err(|e| e.to_string())?;
	}
	
	Ok(())
}

mod cases {
	use crate::modules::Case;
	use linked_hash_map::LinkedHashMap;

	pub fn cases() -> LinkedHashMap<&'static str, Vec<Case>> {
		vec![
			(
				"s2qr",
				vec![Case {
					desc: "Generate QR code for 'hello'".to_string(),
					input: vec!["hello".to_string()],
					output: vec![],
					is_example: true,
					is_test: false, // Output is binary, hard to test with string comparison
					since: "0.13.0".to_string(),
				}],
			),
			(
				"qr2s",
				vec![Case {
					desc: "Scan QR code from camera (interactive)".to_string(),
					input: vec![],
					output: vec![],
					is_example: false,
					is_test: false, // Interactive web-based command, cannot be tested automatically
					since: "0.15.0".to_string(),
				}],
			),
		]
		.into_iter()
		.collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::modules::base::test::test_module;

	#[test]
	fn test_cases() {
		test_module(module());
	}
}
