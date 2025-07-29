use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use opencv::core::Rect;
use opencv::core::Size_;
use opencv::highgui;
use opencv::imgproc;
use opencv::prelude::*;
use opencv::videoio;

use anyhow::{Result, anyhow};
use colored::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InGameTime {
    pub percent: u32,
    pub duration: Duration,
}

impl InGameTime {
    /// Parses a string like ": 117% 3:03:23" into an `InGameTime`
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Remove optional leading ':'
        let s = if let Some(rest) = s.strip_prefix(':') {
            rest.trim()
        } else {
            s
        };

        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(anyhow!("Expected two parts: percentage and time"));
        }

        let percent_str = parts[0].trim_end_matches('%');
        let percent: u32 = percent_str.parse()?;

        let time_parts: Vec<&str> = parts[1].split(':').collect();
        if time_parts.len() != 3 {
            return Err(anyhow!("Time must be in H:MM:SS format"));
        }

        let hours: u64 = time_parts[0].parse()?;
        let minutes: u64 = time_parts[1].parse()?;
        let seconds: u64 = time_parts[2].parse()?;

        let duration = Duration::from_secs(hours * 3600 + minutes * 60 + seconds);

        Ok(Self { percent, duration })
    }
}

impl fmt::Display for InGameTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total_secs = self.duration.as_secs();
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;

        write!(
            f,
            "{}% {:01}:{:02}:{:02}",
            self.percent, hours, minutes, seconds
        )
    }
}

#[derive(Debug, Clone)]
pub struct Split {
    pub percent: u32,
    pub name: String,
    pub time: InGameTime,
}

#[derive(Debug)]
pub struct Splits {
    pub splits: Vec<Split>,
}

impl Splits {
    /// Constructs `Splits` with some placeholder test data.
    pub fn new() -> Self {
        macro_rules! splits {
            ( $( ($percent:expr, $name:expr, $h:expr, $m:expr, $s:expr) ),* $(,)? ) => {
                vec![
                    $(
                        Split {
                            percent: $percent,
                            name: $name.to_string(),
                            time: InGameTime {
                                percent: $percent,
                                duration: std::time::Duration::from_secs($h * 3600 + $m * 60 + $s),
                            },
                        }
                    ),*
                ]
            };
        }

        let splits = splits![
            (18, "Buzz", 0, 25, 43),
            (21, "Crawdad Farm", 0, 28, 15),
            (35, "Enchanted Towers", 0, 55, 46),
            (56, "Fireworks Factory 1", 1, 37, 48),
            (59, "Scorch", 1, 39, 15),
            (67, "Spider Town", 1, 53, 53),
            (70, "Starfish Reef", 1, 57, 23),
            (84, "Agent 9's Lab", 2, 15, 55),
            (85, "Cloud Spires 2", 2, 17, 37),
            // Skipped both 87% entries as requested
            (88, "Fireworks Factory 2", 2, 30, 18),
            (117, "Super Bonus Round", 3, 2, 25)
        ];

        Splits { splits }
    }

    /// Returns the split matching the given percent, if found.
    pub fn find_by_percent(&self, time: &InGameTime) -> Option<&Split> {
        self.splits.iter().find(|s| s.percent == time.percent)
    }

    pub fn compare_and_print(&self, current: &InGameTime) {
        if let Some(split) = self.find_by_percent(current) {
            let delta = current.duration.as_secs() as i64 - split.time.duration.as_secs() as i64;

            let delta_duration = Duration::from_secs(delta.unsigned_abs());
            let colored_delta = if delta > 0 {
                let delta_str = format!(
                    "+{:02}:{:02}",
                    delta_duration.as_secs() / 60,
                    delta_duration.as_secs() % 60
                );
                delta_str.red()
            } else {
                let delta_str = format!(
                    "-{:02}:{:02}",
                    delta_duration.as_secs() / 60,
                    delta_duration.as_secs() % 60
                );
                delta_str.green()
            };

            let current_str = Self::format_time(current.duration);
            println!("{:<22} {:>8} {:>8}", split.name, colored_delta, current_str);
        }
    }

    fn format_time(duration: Duration) -> String {
        let secs = duration.as_secs();
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;
        format!("{:01}:{:02}:{:02}", hours, minutes, seconds)
    }
}

struct Template {
    template: Mat,
    size: Size_<i32>,
    threshold: f32,
    character: char,
}

impl Template {
    pub fn load_from_file(path: &str, threshold: f32, character: char) -> Result<Self> {
        let template = opencv::imgcodecs::imread(path, opencv::imgcodecs::IMREAD_GRAYSCALE)?;
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

        // TODO: store in proper size
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

        Ok(Self {
            template: template_scaled,
            size: template_size,
            threshold,
            character,
        })
    }
}

#[derive(Hash, Eq, PartialEq)]
enum Character {
    Percent,
    Colon,
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
}

struct Templates {
    indices: HashMap<Character, usize>,
    templates: Vec<Template>,
}

impl Templates {
    pub fn load() -> Result<Self> {
        let mut templates = vec![];
        let mut indices = HashMap::new();

        macro_rules! load_template {
            ($char_enum:ident, $filename:expr, $threshold:expr, $display_char:expr) => {{
                let template = Template::load_from_file(
                    concat!("templates/", $filename),
                    $threshold,
                    $display_char,
                )?;
                indices.insert(Character::$char_enum, templates.len());
                templates.push(template);
            }};
        }

        load_template!(Percent, "percent.png", 0.80, '%');
        load_template!(Colon, "colon.png", 0.75, ':');
        load_template!(Zero, "zero.png", 0.80, '0');
        load_template!(One, "one.png", 0.83, '1');
        load_template!(Two, "two.png", 0.83, '2');
        load_template!(Three, "three.png", 0.83, '3');
        load_template!(Four, "four.png", 0.85, '4');
        load_template!(Five, "five.png", 0.85, '5');
        load_template!(Six, "six.png", 0.83, '6');
        load_template!(Seven, "seven.png", 0.85, '7');
        load_template!(Eight, "eight.png", 0.80, '8');
        load_template!(Nine, "nine.png", 0.80, '9');

        Ok(Self { indices, templates })
    }

    pub fn get(&self, character: Character) -> Option<&Template> {
        self.indices
            .get(&character)
            .map(|&idx| &self.templates[idx])
    }
}

#[derive(Clone)]
struct TemplateMatch {
    x: i32,
    y: i32,
    bounding_box: Size_<i32>,
    character: char,
    confidence: f32,
}

fn find_occurances_of_template(
    image: &Mat,
    template: &Template,
    matches: &mut Vec<TemplateMatch>,
) -> Result<()> {
    let result_cols = image.cols() - template.size.width + 1;
    let result_rows = image.rows() - template.size.height + 1;

    let mut result = Mat::new_rows_cols_with_default(
        result_rows,
        result_cols,
        opencv::core::CV_32FC1,
        opencv::core::Scalar::all(0.0),
    )?;

    imgproc::match_template(
        &image,
        &template.template,
        &mut result,
        imgproc::TM_CCOEFF_NORMED,
        &opencv::core::no_array(),
    )?;

    let mut max_val = 0.0;
    for y in 0..result.rows() {
        for x in 0..result.cols() {
            let val = *result.at_2d::<f32>(y, x)?;
            if val >= max_val {
                max_val = val;
            }
            if val >= template.threshold {
                matches.push(TemplateMatch {
                    x,
                    y,
                    bounding_box: template.size,
                    character: template.character,
                    confidence: val,
                });
            }
        }
    }
    //println!("max val: {}", max_val);

    Ok(())
}

fn extract_igt(
    image: &Mat,
    templates: &Templates,
    matches: &mut Vec<TemplateMatch>,
) -> Result<InGameTime> {
    // Use '%' as an indicator whether we are in the guidebook and terminate early if not
    let percent = templates.get(Character::Percent).unwrap();
    find_occurances_of_template(image, &percent, matches)?;

    if matches.is_empty() {
        return Err(anyhow!("No IGT found"));
    }

    // Find occurances of all characters
    for template in &templates.templates {
        if template.character == '%' {
            continue;
        }

        find_occurances_of_template(image, &template, matches)?;
    }

    // Sort by x-coordinate
    matches.sort_by(|a, b| a.x.cmp(&b.x));

    // Simple 1D NMS on x-axis
    let mut filtered: Vec<TemplateMatch> = Vec::new();
    for m in matches.drain(..) {
        let mut replaced = false;
        for other in &mut filtered {
            let m_start = m.x;
            let m_end = m.x + m.bounding_box.width;
            let o_start = other.x;
            let o_end = other.x + other.bounding_box.width;

            let overlap = (m_end.min(o_end) - m_start.max(o_start)).max(0);
            let min_width = m.bounding_box.width.min(other.bounding_box.width);

            if overlap as f32 > 0.5 * min_width as f32 {
                if m.confidence > other.confidence {
                    *other = m.clone();
                }
                replaced = true;
                break;
            }
        }

        if !replaced {
            filtered.push(m);
        }
    }

    // Sort again to ensure left-to-right order
    filtered.sort_by(|a, b| a.x.cmp(&b.x));

    let mut result = String::new();
    for (i, m) in filtered.iter().enumerate() {
        if i > 0 {
            let prev = &filtered[i - 1];
            let gap = m.x - (prev.x + prev.bounding_box.width);
            if gap as f32 > 20.0 {
                result.push(' ');
            }
        }
        result.push(m.character);
    }

    // Return filtered matches to caller
    *matches = filtered;

    Ok(InGameTime::parse(&result)?)
}

fn main() -> Result<()> {
    let debug = false;

    let mut video = videoio::VideoCapture::new(2, videoio::CAP_ANY)?;
    /*let mut video =
    videoio::VideoCapture::from_file_def("C:\\Users\\domin\\Videos\\2025-07-16 20-00-51.mkv")?;*/
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

    if debug {
        highgui::named_window("Webcam OCR", highgui::WINDOW_NORMAL)?;
    }

    // Define the region of interest (ROI)
    let roi_rect = Rect::new(1260, 45, 620, 50); // x, y, width, height

    // Load template images
    let templates = Templates::load()?;

    let splits = Splits::new();

    let mut resized = false;
    let mut last_igt = InGameTime::default();

    println!();
    println!();
    println!();
    loop {
        let mut frame = Mat::default();
        video.read(&mut frame)?;
        if frame.empty() {
            continue;
        }

        //let now = Instant::now();
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

        let mut matches: Vec<TemplateMatch> = vec![];
        if let Ok(igt) = extract_igt(&binarized_roi, &templates, &mut matches) {
            //let elapsed = now.elapsed();
            //println!("Found <{}> in {} ms", igt, elapsed.as_millis());

            if igt != last_igt {
                //println!("IGT: {}", igt);
                splits.compare_and_print(&igt);
                last_igt = igt;
            }
        }

        if debug {
            for pt in matches {
                let top_left = opencv::core::Point::new(roi_rect.x + pt.x, roi_rect.y + pt.y);
                opencv::imgproc::rectangle(
                    &mut frame,
                    opencv::core::Rect::new(
                        top_left.x,
                        top_left.y,
                        pt.bounding_box.width,
                        pt.bounding_box.height,
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
                let _ = highgui::resize_window(
                    "Webcam OCR",
                    display_frame.cols(),
                    display_frame.rows(),
                )?;
                resized = true;
            }

            highgui::imshow("Webcam OCR", &display_frame)?;
            if highgui::wait_key(1)? == 27 {
                break; // ESC to quit
            }
        }
    }

    Ok(())
}
