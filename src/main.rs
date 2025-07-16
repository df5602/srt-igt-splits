use std::time::Instant;

use opencv::core::Point_;
use opencv::core::Rect;
use opencv::highgui;
use opencv::imgproc;
use opencv::prelude::*;
use opencv::videoio;

use anyhow::Result;

fn find_occurances_of_template(
    image: &Mat,
    template: &Mat,
    threshold: f32,
    matches: &mut Vec<Point_<i32>>,
) -> Result<()> {
    let template_size = template.size()?;

    let result_cols = image.cols() - template_size.width + 1;
    let result_rows = image.rows() - template_size.height + 1;

    let mut result = Mat::new_rows_cols_with_default(
        result_rows,
        result_cols,
        opencv::core::CV_32FC1,
        opencv::core::Scalar::all(0.0),
    )?;

    imgproc::match_template(
        &image,
        &template,
        &mut result,
        imgproc::TM_CCOEFF_NORMED,
        &opencv::core::no_array(),
    )?;

    let mut result_norm = Mat::default();
    opencv::core::normalize(
        &result,
        &mut result_norm,
        0.0,
        255.0,
        opencv::core::NORM_MINMAX,
        opencv::core::CV_8UC1,
        &opencv::core::no_array(),
    )?;

    let mut result_color = Mat::default();
    opencv::imgproc::apply_color_map(
        &result_norm,
        &mut result_color,
        opencv::imgproc::COLORMAP_JET,
    )?;

    let mut result_scaled = Mat::default();
    opencv::imgproc::resize(
        &result_color,
        &mut result_scaled,
        opencv::core::Size {
            width: result_color.cols() * 2,
            height: result_color.rows() * 2,
        },
        0.0,
        0.0,
        imgproc::INTER_LINEAR,
    )?;

    opencv::highgui::imshow("Match Heatmap", &result_scaled)?;
    opencv::highgui::imshow("Image (binarized)", &image)?;
    opencv::highgui::imshow("Template (binarized)", &template)?;
    //highgui::wait_key(0)?;

    let mut min_val = 0.0;
    let mut max_val = 0.0;
    let mut min_loc = opencv::core::Point::default();
    let mut max_loc = opencv::core::Point::default();

    opencv::core::min_max_loc(
        &result,
        Some(&mut min_val),
        Some(&mut max_val),
        Some(&mut min_loc),
        Some(&mut max_loc),
        &opencv::core::no_array(),
    )?;

    // Print max match value (best score)
    println!("Template match max score: {:.4} at {:?}", max_val, max_loc);

    for y in 0..result.rows() {
        for x in 0..result.cols() {
            let val = *result.at_2d::<f32>(y, x)?;
            if val >= threshold {
                matches.push(opencv::core::Point::new(x, y));
                println!("Found match with score {} at ({},{})", val, x, y);
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    //let mut video = videoio::VideoCapture::new(2, videoio::CAP_ANY)?;
    let mut video =
        videoio::VideoCapture::from_file_def("C:\\Users\\xxx\\Videos\\2025-07-16 20-00-51.mkv")?;
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

    // Load template image
    // ==== EXPERIMENT ====
    let template =
        opencv::imgcodecs::imread("templates/nine.png", opencv::imgcodecs::IMREAD_GRAYSCALE)?;
    if template.empty() {
        panic!("Failed to load template!");
    }

    let mut binarized_template = Mat::default();
    opencv::imgproc::threshold(
        &template,
        &mut binarized_template,
        0.0,
        255.0,
        imgproc::THRESH_OTSU,
    )?;

    // TODO: calculate proper scaling factor (ration between 1080p and screen res) or ideally store scaled properly
    let mut template_scaled = Mat::default();
    opencv::imgproc::resize(
        &binarized_template,
        &mut template_scaled,
        opencv::core::Size {
            width: (binarized_template.cols() as f32 * 0.75) as i32,
            height: (binarized_template.rows() as f32 * 0.75) as i32,
        },
        0.0,
        0.0,
        imgproc::INTER_LINEAR,
    )?;

    let template_size = template_scaled.size()?;
    println!(
        "Template size: {} x {}",
        template_size.width, template_size.height
    );

    /*let _ = highgui::resize_window("Webcam OCR", template.cols(), template.rows())?;
    highgui::imshow("Webcam OCR", &template)?;
    highgui::wait_key(0)?;*/

    let threshold = 0.85f32;

    // Colon: colon.png, thr: 0.80
    // Percent: percent.png, thr: 0.85
    // Zero: zero.png, thr: 0.85
    // One: one.png, thr: 0.85
    // Two: two.png, thr: 0.85
    // Three: three.png, thr: 0.85
    // Four: four.png, thr: 0.85
    // Five: five.png, thr: 0.85
    // Six: six.png, thr: 0.85
    // Seven: seven.png, thr: 0.9
    // Eight: eight.png, thr: 0.85
    // Nine: nine.png, thr: 0.85
    // ==== END EXPERIMENT ====

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

        let mut binarized_roi = Mat::default();
        opencv::imgproc::threshold(
            &gray,              // input
            &mut binarized_roi, // output
            0.0,                // threshold value (0 = auto for Otsu)
            255.0,              // max value
            imgproc::THRESH_OTSU,
        )?;

        let mut matches: Vec<Point_<i32>> = vec![];
        find_occurances_of_template(&binarized_roi, &template_scaled, threshold, &mut matches)?;
        let elapsed = now.elapsed();
        println!("Template matching took {} ms", elapsed.as_millis());

        for pt in matches {
            let top_left = opencv::core::Point::new(roi_rect.x + pt.x, roi_rect.y + pt.y);
            opencv::imgproc::rectangle(
                &mut frame,
                opencv::core::Rect::new(
                    top_left.x,
                    top_left.y,
                    template_size.width,
                    template_size.height,
                ),
                opencv::core::Scalar::new(255.0, 0.0, 255.0, 0.0),
                2,
                imgproc::LINE_8,
                0,
            )?;
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

        let mut display_frame = Mat::default();
        opencv::imgproc::resize(
            &frame,
            &mut display_frame,
            opencv::core::Size {
                width: frame.cols() / 2,
                height: frame.rows() / 2,
            },
            0.0,
            0.0,
            imgproc::INTER_LINEAR,
        )?;

        if !resized {
            println!("Frame: {} x {}", display_frame.cols(), display_frame.rows());
            let _ =
                highgui::resize_window("Webcam OCR", display_frame.cols(), display_frame.rows())?;
            resized = true;
        }

        highgui::imshow("Webcam OCR", &display_frame)?;
        if highgui::wait_key(1)? == 27 {
            break; // ESC to quit
        }
    }

    Ok(())
}
