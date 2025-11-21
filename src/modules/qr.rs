use crate::modules::{base, Command, Module};
use clap::{Arg, ArgMatches, SubCommand};
use image::Luma;
use qrcode::QrCode;
use std::io::{self, Cursor, Read, Write};

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
	let mut buffer = Vec::new();
	io::stdin()
		.read_to_end(&mut buffer)
		.map_err(|e| format!("Failed to read from stdin: {}", e))?;

	let img = image::load_from_memory(&buffer).map_err(|e| format!("Failed to load image: {}", e))?;
	let img = img.to_luma8();

	let mut img = rqrr::PreparedImage::prepare(img);
	let grids = img.detect_grids();

	if grids.is_empty() {
		return Err("No QR code detected".to_string());
	}

	let mut result = Vec::new();
	for grid in grids {
		let (_meta, content) = grid.decode().map_err(|e| format!("Failed to decode QR code: {}", e))?;
		result.push(content);
	}

	Ok(result)
}

mod cases {
	use crate::modules::Case;
	use linked_hash_map::LinkedHashMap;

	pub fn cases() -> LinkedHashMap<&'static str, Vec<Case>> {
		vec![(
			"s2qr",
			vec![Case {
				desc: "Generate QR code for 'hello'".to_string(),
				input: vec!["hello".to_string()],
				output: vec![],
				is_example: true,
				is_test: false, // Output is binary, hard to test with string comparison
				since: "0.13.0".to_string(),
			}],
		)]
		.into_iter()
		.collect()
	}
}
