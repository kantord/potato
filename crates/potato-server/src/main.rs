use tokio::net::UnixListener;

const APPS: &[&str] = &["potato-hello-world", "potato-hello-simple"];

struct RunningApp {
    name: String,
    container_id: Option<String>,
}

#[tokio::main]
async fn main() {
    let mut running_apps = Vec::new();

    for &image in APPS {
        println!("[{image}] Extracting image filesystem...");
        let static_dir = match potato_server::extract_image(image).await {
            Ok(dir) => {
                println!("[{image}] Extracted to {}", dir.display());
                dir
            }
            Err(e) => {
                eprintln!("[{image}] Failed to extract image: {e}");
                continue;
            }
        };

        let container_id = match potato_server::start_container(image).await {
            Ok(id) => {
                println!("[{image}] Container {id} running");
                Some(id)
            }
            Err(e) => {
                println!("[{image}] No container started (static-only app): {e}");
                None
            }
        };

        let path = format!("/tmp/potato-{image}.sock");
        let _ = std::fs::remove_file(&path);

        let listener = UnixListener::bind(&path).unwrap();
        println!("[{image}] Listening on {path}");

        let app = potato_server::app(static_dir, container_id.clone());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        running_apps.push(RunningApp {
            name: image.to_string(),
            container_id,
        });
    }

    println!("All apps started. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await.unwrap();

    println!("Shutting down...");
    for app in &running_apps {
        if let Some(id) = &app.container_id {
            println!("[{}] Stopping container...", app.name);
            potato_server::stop_container(id).await;
        }
    }
}
