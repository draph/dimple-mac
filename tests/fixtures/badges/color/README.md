# Color import fixtures

These test-only images were created for this project; they contain no third-party assets or private metadata.

`rgb.jpg` is a 24×16 red/green/blue/yellow quadrant image, encoded with ImageMagick at quality 95 and 4:4:4 sampling. `progressive.jpeg`, `gray.jpg` and `cmyk.jpg` are its progressive, grayscale and CMYK conversions. JPEG tests add minimal EXIF orientation blocks in memory, exercise all eight transforms and check metadata removal and bounded rejection. CMYK is explicitly rejected: only RGB and grayscale JPEG color models are accepted.

`color.svg` combines transparent margins, an opaque red rectangle and a half-transparent green rectangle. It exercises straight RGBA normalization, original-color compositing and the engraved Alpha path at all seven output sizes. No color keying or background removal is performed.
