export type SignatureInkColour = "original" | "black" | "blue";

export type ProcessedSignature = {
  dataUrl: string;
  height: number;
  sourceName: string;
  width: number;
};

export type SignatureProcessingOptions = {
  feather: number;
  inkColour: SignatureInkColour;
  padding: number;
  tolerance: number;
};

export type TypedSignatureStyle = "classic" | "modern" | "script";

export type SignatureArtworkErrorCode =
  | "crop-image"
  | "crop-visual"
  | "no-image-ink"
  | "no-visual-ink"
  | "open-image"
  | "prepare-image"
  | "prepare-typed"
  | "prepare-visual"
  | "typed-length";

export class SignatureArtworkError extends Error {
  readonly code: SignatureArtworkErrorCode;

  constructor(code: SignatureArtworkErrorCode, message: string) {
    super(message);
    this.name = "SignatureArtworkError";
    this.code = code;
  }
}

type Rgb = {
  blue: number;
  green: number;
  red: number;
};

export async function processSignatureImage(
  file: File,
  options: SignatureProcessingOptions
): Promise<ProcessedSignature> {
  const image = await loadImage(file);
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d", { willReadFrequently: true });

  if (!context) {
    throw new SignatureArtworkError(
      "prepare-image",
      "This device could not prepare the signature image."
    );
  }

  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  context.drawImage(image, 0, 0);

  const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
  const background = estimateBackground(imageData, canvas.width, canvas.height);
  const bounds = removeBackground(imageData, background, options);

  if (!bounds) {
    throw new SignatureArtworkError(
      "no-image-ink",
      "No signature ink was detected. Try lowering the background removal setting."
    );
  }

  context.putImageData(imageData, 0, 0);

  const left = Math.max(0, bounds.left - options.padding);
  const top = Math.max(0, bounds.top - options.padding);
  const right = Math.min(canvas.width, bounds.right + options.padding + 1);
  const bottom = Math.min(canvas.height, bounds.bottom + options.padding + 1);
  const width = right - left;
  const height = bottom - top;
  const output = document.createElement("canvas");
  const outputContext = output.getContext("2d");

  if (!outputContext) {
    throw new SignatureArtworkError(
      "crop-image",
      "This device could not crop the signature image."
    );
  }

  output.width = width;
  output.height = height;
  outputContext.drawImage(canvas, left, top, width, height, 0, 0, width, height);

  return {
    dataUrl: output.toDataURL("image/png"),
    height,
    sourceName: file.name,
    width
  };
}

export function createTypedSignature(
  value: string,
  style: TypedSignatureStyle,
  inkColour: Exclude<SignatureInkColour, "original">,
  sourceName: string
): ProcessedSignature {
  const text = value.trim();
  if (!text || text.length > 80) {
    throw new SignatureArtworkError(
      "typed-length",
      "Typed signatures and initials must contain between 1 and 80 characters."
    );
  }
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    throw new SignatureArtworkError(
      "prepare-typed",
      "This device could not prepare typed signature artwork."
    );
  }
  const font = typedSignatureFont(style);
  context.font = font;
  const metrics = context.measureText(text);
  const padding = 28;
  const measuredHeight = Math.ceil(
    metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent
  );
  canvas.width = Math.max(120, Math.min(2_048, Math.ceil(metrics.width) + padding * 2));
  canvas.height = Math.max(96, Math.min(512, measuredHeight + padding * 2));
  context.font = font;
  context.fillStyle = inkColour === "blue" ? "#1844a6" : "#181b20";
  context.textBaseline = "alphabetic";
  context.fillText(text, padding, padding + metrics.actualBoundingBoxAscent);
  return processedSignatureFromCanvas(canvas, sourceName, 8);
}

export function processedSignatureFromCanvas(
  canvas: HTMLCanvasElement,
  sourceName: string,
  padding = 8
): ProcessedSignature {
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context || canvas.width < 1 || canvas.height < 1) {
    throw new SignatureArtworkError(
      "prepare-visual",
      "This device could not prepare the visual signature."
    );
  }
  const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
  let left = canvas.width;
  let top = canvas.height;
  let right = -1;
  let bottom = -1;
  for (let y = 0; y < canvas.height; y += 1) {
    for (let x = 0; x < canvas.width; x += 1) {
      const alpha = imageData.data[(y * canvas.width + x) * 4 + 3];
      if (alpha > 8) {
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x);
        bottom = Math.max(bottom, y);
      }
    }
  }
  if (right < left || bottom < top) {
    throw new SignatureArtworkError("no-visual-ink", "No signature ink was detected.");
  }
  const cropLeft = Math.max(0, left - padding);
  const cropTop = Math.max(0, top - padding);
  const cropRight = Math.min(canvas.width, right + padding + 1);
  const cropBottom = Math.min(canvas.height, bottom + padding + 1);
  const output = document.createElement("canvas");
  const outputContext = output.getContext("2d");
  if (!outputContext) {
    throw new SignatureArtworkError(
      "crop-visual",
      "This device could not crop the visual signature."
    );
  }
  output.width = cropRight - cropLeft;
  output.height = cropBottom - cropTop;
  outputContext.drawImage(
    canvas,
    cropLeft,
    cropTop,
    output.width,
    output.height,
    0,
    0,
    output.width,
    output.height
  );
  return {
    dataUrl: output.toDataURL("image/png"),
    height: output.height,
    sourceName,
    width: output.width
  };
}

async function loadImage(file: File) {
  const url = URL.createObjectURL(file);

  try {
    return await new Promise<HTMLImageElement>((resolve, reject) => {
      const image = new Image();
      image.onload = () => resolve(image);
      image.onerror = () =>
        reject(
          new SignatureArtworkError(
            "open-image",
            "The selected image format could not be opened."
          )
        );
      image.src = url;
    });
  } finally {
    URL.revokeObjectURL(url);
  }
}

function estimateBackground(imageData: ImageData, width: number, height: number): Rgb {
  const sampleSize = Math.max(2, Math.min(12, Math.floor(Math.min(width, height) * 0.04)));
  const samples: Rgb[] = [];
  const corners = [
    [0, 0],
    [width - sampleSize, 0],
    [0, height - sampleSize],
    [width - sampleSize, height - sampleSize]
  ];

  for (const [startX, startY] of corners) {
    for (let y = startY; y < startY + sampleSize; y += 1) {
      for (let x = startX; x < startX + sampleSize; x += 1) {
        const offset = (y * width + x) * 4;
        samples.push({
          red: imageData.data[offset],
          green: imageData.data[offset + 1],
          blue: imageData.data[offset + 2]
        });
      }
    }
  }

  const total = samples.reduce(
    (sum, sample) => ({
      red: sum.red + sample.red,
      green: sum.green + sample.green,
      blue: sum.blue + sample.blue
    }),
    { red: 0, green: 0, blue: 0 }
  );

  return {
    red: total.red / samples.length,
    green: total.green / samples.length,
    blue: total.blue / samples.length
  };
}

function removeBackground(
  imageData: ImageData,
  background: Rgb,
  options: SignatureProcessingOptions
) {
  let left = imageData.width;
  let top = imageData.height;
  let right = -1;
  let bottom = -1;

  for (let y = 0; y < imageData.height; y += 1) {
    for (let x = 0; x < imageData.width; x += 1) {
      const offset = (y * imageData.width + x) * 4;
      const red = imageData.data[offset];
      const green = imageData.data[offset + 1];
      const blue = imageData.data[offset + 2];
      const sourceAlpha = imageData.data[offset + 3];
      const distance = Math.sqrt(
        (red - background.red) ** 2 +
          (green - background.green) ** 2 +
          (blue - background.blue) ** 2
      );
      const strength = clamp((distance - options.tolerance) / options.feather, 0, 1);
      const alpha = Math.round(sourceAlpha * strength);

      imageData.data[offset + 3] = alpha;

      if (alpha < 10) {
        imageData.data[offset + 3] = 0;
        continue;
      }

      if (options.inkColour === "black") {
        imageData.data[offset] = 24;
        imageData.data[offset + 1] = 27;
        imageData.data[offset + 2] = 32;
      } else if (options.inkColour === "blue") {
        imageData.data[offset] = 24;
        imageData.data[offset + 1] = 68;
        imageData.data[offset + 2] = 166;
      }

      left = Math.min(left, x);
      top = Math.min(top, y);
      right = Math.max(right, x);
      bottom = Math.max(bottom, y);
    }
  }

  return right >= left && bottom >= top ? { bottom, left, right, top } : null;
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}

function typedSignatureFont(style: TypedSignatureStyle) {
  if (style === "classic") {
    return 'italic 82px Georgia, "Times New Roman", serif';
  }
  if (style === "modern") {
    return '600 72px "Segoe UI", Arial, sans-serif';
  }
  return 'italic 88px "Segoe Script", "Brush Script MT", cursive';
}
