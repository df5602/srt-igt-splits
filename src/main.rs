use std::time::Instant;

use opencv::core::Rect;
use opencv::highgui;
use opencv::imgproc;
use opencv::prelude::*;
use opencv::videoio;
use tesseract::ocr_from_frame;

use anyhow::Result;

fn main() -> Result<()> {
    let version = opencv::core::CV_VERSION;
    println!("OpenCV version: {}", version);

    let mut video = videoio::VideoCapture::new(2, videoio::CAP_ANY)?;
    if !videoio::VideoCapture::is_opened(&video)? {
        panic!("Unable to open video!");
    }

    // Set resolution to 1920x1080
    video.set(opencv::videoio::CAP_PROP_FRAME_WIDTH, 1920.0)?;
    video.set(opencv::videoio::CAP_PROP_FRAME_HEIGHT, 1080.0)?;

    // Optional: read back to verify
    let width = video.get(opencv::videoio::CAP_PROP_FRAME_WIDTH)?;
    let height = video.get(opencv::videoio::CAP_PROP_FRAME_HEIGHT)?;
    println!("Resolution set to: {}x{}", width, height);

    highgui::named_window("Webcam OCR", highgui::WINDOW_NORMAL)?;

    // Define the region of interest (ROI)
    let roi_rect = Rect::new(1260, 45, 620, 50); // x, y, width, height

    let mut resized = false;
    loop {
        let mut frame = Mat::default();
        video.read(&mut frame)?;
        if frame.empty() {
            continue;
        }

        let now = Instant::now();
        let roi_view = Mat::roi(&frame, roi_rect)?;
        let mut roi = Mat::default();
        opencv::core::copy_to(&roi_view, &mut roi, &opencv::core::no_array())?;

        // Convert ROI to grayscale
        let mut gray = Mat::default();
        imgproc::cvt_color(
            &roi,
            &mut gray,
            imgproc::COLOR_BGR2GRAY,
            0,
            opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT,
        )?;

        let mut scaled = Mat::default();
        let scale_factor = 1.0;

        imgproc::resize(
            &gray,
            &mut scaled,
            opencv::core::Size {
                width: (gray.cols() as f64 * scale_factor) as i32,
                height: (gray.rows() as f64 * scale_factor) as i32,
            },
            0.0,
            0.0,
            imgproc::INTER_CUBIC,
        )?;

        // Get image parameters
        let width = gray.cols();
        let height = gray.rows();
        let bytes_per_pixel = gray.channels();
        let bytes_per_line = gray.step1(0)? as i32;

        // Get the raw image data
        let frame_data = gray.data_bytes()?;

        // Perform OCR
        match ocr_from_frame(
            frame_data,
            width,
            height,
            bytes_per_pixel,
            bytes_per_line,
            "eng",
        ) {
            Ok(text) => {
                let elapsed_time = now.elapsed();
                println!(
                    "OCR Output [{} ms]: {}",
                    elapsed_time.as_millis(),
                    text.trim()
                );
            }
            Err(e) => eprintln!("OCR error: {:?}", e),
        }

        // Draw ROI rectangle on original frame
        opencv::imgproc::rectangle(
            &mut frame,
            roi_rect,
            opencv::core::Scalar::new(0.0, 255.0, 0.0, 0.0),
            2,
            imgproc::LINE_8,
            0,
        )?;

        if !resized {
            println!("Frame: {} x {}", frame.cols(), frame.rows());
            let _ = highgui::resize_window("Webcam OCR", frame.cols(), frame.rows())?;
            resized = true;
        }

        highgui::imshow("Webcam OCR", &frame)?;
        if highgui::wait_key(1)? == 27 {
            break; // ESC to quit
        }
    }

    Ok(())
}
