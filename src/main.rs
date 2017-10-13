extern crate cairo;
use std::fs::File;

fn main() {
    let surface = cairo::ImageSurface::create(cairo::Format::Rgb24, 500, 500)
                  .expect("Could not create a Cairo surface");

    let ctx = cairo::Context::new(&surface);

    ctx.set_line_width(0.8);
    ctx.set_source_rgb(0.5, 0.0, 0.0);
    ctx.rectangle(0.25, 0.25, 300.0, 300.0);
    ctx.stroke();

    let mut buffer = File::create("img.png").unwrap();
    surface.write_to_png(&mut buffer).unwrap();
}
