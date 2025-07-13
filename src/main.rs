//use rusty_tesseract::Image;
//use rusty_tesseract::Args;

use std::time::Instant;

use tesseract::ocr;

fn main() {
    /*let img = Image::from_path("guidebook_1.png").unwrap();
    let img_cropped = Image::from_path("guidebook_1_cropped.png").unwrap();
    let default_args = Args::default();

    // string output
    let now = Instant::now();
    let output = rusty_tesseract::image_to_string(&img, &default_args).unwrap();
    let elapsed_time = now.elapsed();
    println!(
        "Running rusty_tesseract::image_to_string() on full image took {} ms.",
        elapsed_time.as_millis()
    );
    println!("The String output is: {:?}", output);

    let now = Instant::now();
    let output_cropped = rusty_tesseract::image_to_string(&img_cropped, &default_args).unwrap();
    let elapsed_time = now.elapsed();
    println!(
        "Running rusty_tesseract::image_to_string() on cropped image took {} ms.",
        elapsed_time.as_millis()
    );
    println!("The String output is: {:?}", output_cropped);*/

    let now = Instant::now();
    let output = ocr("guidebook_1.png", "eng");
    let elapsed_time = now.elapsed();
    println!(
        "Running ocr() on full image took {} ms.",
        elapsed_time.as_millis()
    );
    println!("The string output is: {}", output.unwrap());

    let now = Instant::now();
    let output = ocr("guidebook_1_cropped.png", "eng");
    let elapsed_time = now.elapsed();
    println!(
        "Running ocr() on cropped image took {} ms.",
        elapsed_time.as_millis()
    );
    println!("The string output is: {}", output.unwrap());

    let now = Instant::now();
    let output = ocr("guidebook_1_black.png", "eng");
    let elapsed_time = now.elapsed();
    println!(
        "Running ocr() on black image took {} ms.",
        elapsed_time.as_millis()
    );
    println!("The string output is: {}", output.unwrap());

    let now = Instant::now();
    let output = ocr("guidebook_1_messy.png", "eng");
    let elapsed_time = now.elapsed();
    println!(
        "Running ocr() on messy image took {} ms.",
        elapsed_time.as_millis()
    );
    println!("The string output is: {}", output.unwrap());
}
