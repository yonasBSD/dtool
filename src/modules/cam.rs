use crate::modules::{Command, Module};
use clap::{Arg, ArgMatches, SubCommand};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use std::io::{self, Write};
use minifb::{Key, Window, WindowOptions};

pub fn module<'a, 'b>() -> Module<'a, 'b> {
	Module {
		desc: "Camera capture".to_string(),
		commands: commands(),
		get_cases: cases::cases,
	}
}

pub fn commands<'a, 'b>() -> Vec<Command<'a, 'b>> {
	vec![
		Command {
			app: SubCommand::with_name("cam")
				.about("Camera operations")
				.subcommand(SubCommand::with_name("list").about("List available cameras"))
				.subcommand(
					SubCommand::with_name("capture")
						.about("Capture a frame")
						.arg(
							Arg::with_name("index")
								.long("index")
								.short("i")
								.takes_value(true)
						.help("Camera index (default: 0)"),
						),
				),
			f: run_cam,
		},
	]
}

fn run_cam(matches: &ArgMatches) -> Result<Vec<String>, String> {
	if let Some(_) = matches.subcommand_matches("list") {
		return list_cameras();
	}

	if let Some(matches) = matches.subcommand_matches("capture") {
		return capture_frame(matches);
	}

	Err("Subcommand required".to_string())
}

fn list_cameras() -> Result<Vec<String>, String> {
	let cameras = nokhwa::query(nokhwa::utils::ApiBackend::Auto).map_err(|e| format!("Failed to query cameras: {}", e))?;

	if cameras.is_empty() {
		return Ok(vec!["No cameras found".to_string()]);
	}

	let mut result = Vec::new();
	for cam in cameras {
		result.push(format!("{}: {}", cam.index(), cam.human_name()));
	}

	Ok(result)
}

fn capture_frame(matches: &ArgMatches) -> Result<Vec<String>, String> {
	let index_str = matches.value_of("index").unwrap_or("0");
	let index: u32 = index_str.parse().map_err(|_| "Invalid camera index")?;

	let camera_index = CameraIndex::Index(index);
	let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

	let mut camera = Camera::new(camera_index, requested).map_err(|e| format!("Failed to open camera: {}", e))?;

	camera.open_stream().map_err(|e| format!("Failed to open stream: {}", e))?;

	let mut window = Window::new(
		"Press SPACE to capture",
		512,
		512,
		WindowOptions::default(),
	)
	.map_err(|e| format!("Unable to create window: {}", e))?;

	// Limit to 60 fps
	window.limit_update_rate(Some(std::time::Duration::from_micros(16600)));

	let mut captured_frame = None;
	let preview_width = 512;
	let preview_height = 512;
	let mut buffer: Vec<u32> = vec![0; preview_width * preview_height];

	while window.is_open() && !window.is_key_down(Key::Escape) {
		let frame = camera.frame().map_err(|e| format!("Failed to capture frame: {}", e))?;
		
		let image = frame.decode_image::<RgbFormat>().map_err(|e| format!("Failed to decode image: {}", e))?;

		let width = image.width();
		let height = image.height();
		let min_dim = std::cmp::min(width, height);
		let x_offset = (width - min_dim) / 2;
		let y_offset = (height - min_dim) / 2;

		// Downscale and mirror to 512x512
		// We iterate over the destination pixels (preview window)
		let raw_pixels = image.as_raw();
		let stride = width as usize * 3; // 3 bytes per pixel

		for y in 0..preview_height {
			// Map destination y to source y
			let src_y = y_offset + (y as u32 * min_dim / preview_height as u32);
			let row_start = src_y as usize * stride;

			for x in 0..preview_width {
				// Map destination x to source x
				// Mirroring: invert x axis relative to the crop
				let src_x_rel = min_dim - 1 - (x as u32 * min_dim / preview_width as u32);
				let src_x = x_offset + src_x_rel;

				let pixel_idx = row_start + src_x as usize * 3;
				
				// Unsafe access for speed? Or just direct indexing which is safe enough here
				// We trust our math, but let's stick to safe indexing for now.
				// Bounds check elimination would be nice but let's see if this is fast enough.
				if pixel_idx + 2 < raw_pixels.len() {
					let r = raw_pixels[pixel_idx] as u32;
					let g = raw_pixels[pixel_idx + 1] as u32;
					let b = raw_pixels[pixel_idx + 2] as u32;
					
					buffer[y * preview_width + x] = 0xFF000000 | (r << 16) | (g << 8) | b;
				}
			}
		}

		window
			.update_with_buffer(&buffer, preview_width, preview_height)
			.map_err(|e| format!("Unable to update window: {}", e))?;

		if window.is_key_down(Key::Space) {
			// Capture the original unmirrored crop
			let mut image_clone = image.clone();
			let cropped = image::imageops::crop(&mut image_clone, x_offset, y_offset, min_dim, min_dim).to_image();
			captured_frame = Some(cropped);
			break;
		}
	}

	let image = captured_frame.ok_or("Capture cancelled")?;

	let mut buffer = std::io::Cursor::new(Vec::new());
	image
		.write_to(&mut buffer, image::ImageFormat::Png)
		.map_err(|e| format!("Failed to write image to buffer: {}", e))?;

	io::stdout()
		.write_all(buffer.get_ref())
		.map_err(|e| format!("Failed to write image to stdout: {}", e))?;

	Ok(vec![])
}

mod cases {
	use crate::modules::Case;
	use linked_hash_map::LinkedHashMap;

	pub fn cases() -> LinkedHashMap<&'static str, Vec<Case>> {
		vec![].into_iter().collect()
	}
}
