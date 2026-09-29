const dropZone = document.querySelector("#drop-zone");
const fileInput = document.querySelector("#file-input");
const editor = document.querySelector("#editor");
const imagePreview = document.querySelector("#image-preview");
const videoPreview = document.querySelector("#video-preview");
const imageOptions = document.querySelector("#image-options");
const videoNote = document.querySelector("#video-note");
const widthInput = document.querySelector("#width-input");
const heightInput = document.querySelector("#height-input");
const aspectLock = document.querySelector("#aspect-lock");
const resizeButton = document.querySelector("#resize-button");
const progressWrap = document.querySelector("#progress-wrap");
const progressBar = document.querySelector("#progress-bar");
const statusText = document.querySelector("#status-text");
const progressValue = document.querySelector("#progress-value");
const errorMessage = document.querySelector("#error-message");
const imageTypes = new Set(["image/avif", "image/bmp", "image/gif", "image/jpeg", "image/png", "image/webp"]);
const videoTypes = new Set(["video/mp4", "video/ogg", "video/quicktime", "video/webm", "video/x-matroska", "video/x-msvideo"]);

let selectedFile;
let previewUrl;
let naturalWidth = 0;
let naturalHeight = 0;
let mediaType = "";

document.querySelector("#browse-button").addEventListener("click", () => fileInput.click());
document.querySelector("#change-file").addEventListener("click", () => fileInput.click());
dropZone.addEventListener("click", (event) => {
  if (event.target.closest("button")) return;
  fileInput.click();
});
dropZone.addEventListener("keydown", (event) => {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    fileInput.click();
  }
});
fileInput.addEventListener("change", () => {
  if (fileInput.files?.[0]) loadFile(fileInput.files[0]);
  fileInput.value = "";
});

for (const eventName of ["dragenter", "dragover"]) {
  dropZone.addEventListener(eventName, (event) => {
    event.preventDefault();
    dropZone.classList.add("is-dragging");
  });
}
for (const eventName of ["dragleave", "drop"]) {
  dropZone.addEventListener(eventName, (event) => {
    event.preventDefault();
    dropZone.classList.remove("is-dragging");
  });
}
dropZone.addEventListener("drop", (event) => {
  const file = event.dataTransfer?.files?.[0];
  if (file) loadFile(file);
});

widthInput.addEventListener("input", () => updateDimension("width"));
heightInput.addEventListener("input", () => updateDimension("height"));
document.querySelector("#quality-input").addEventListener("input", (event) => {
  document.querySelector("#quality-value").textContent = `${event.target.value}%`;
});
document.querySelector("#resize-button").addEventListener("click", resizeAndDownload);

function loadFile(file) {
  const mimeType = file.type.toLowerCase();
  const type = imageTypes.has(mimeType) ? "image" : videoTypes.has(mimeType) ? "video" : "";
  if (!type) {
    showError("Choose a JPG, PNG, WebP, GIF, BMP, AVIF, or supported video file.");
    return;
  }
  if (file.size > 500 * 1024 * 1024) {
    showError("This file is larger than the 500 MB limit.");
    return;
  }
  clearError();
  releasePreviewUrl();
  selectedFile = file;
  mediaType = type;
  previewUrl = URL.createObjectURL(new Blob([file], { type: mimeType }));
  document.querySelector("#file-name").textContent = file.name;
  document.querySelector("#file-meta").textContent = formatBytes(file.size);
  document.querySelector("#media-badge").textContent = type;
  editor.hidden = false;
  dropZone.hidden = true;
  imagePreview.hidden = type !== "image";
  videoPreview.hidden = type !== "video";
  imageOptions.hidden = type !== "image";
  videoNote.hidden = type !== "video";
  document.querySelector("#empty-preview").hidden = true;

  const preview = type === "image" ? imagePreview : videoPreview;
  preview.onload = null;
  preview.onerror = null;
  preview.onloadedmetadata = null;
  if (type === "image") {
    imagePreview.onload = () => setSourceDimensions(imagePreview.naturalWidth, imagePreview.naturalHeight);
    imagePreview.onerror = () => showError("This image could not be opened by your browser.");
    imagePreview.src = previewUrl;
  } else {
    videoPreview.onloadedmetadata = () => setSourceDimensions(videoPreview.videoWidth, videoPreview.videoHeight);
    videoPreview.onerror = () => showError("This video could not be opened by your browser.");
    videoPreview.src = previewUrl;
    videoPreview.load();
  }
}

function setSourceDimensions(width, height) {
  naturalWidth = width;
  naturalHeight = height;
  widthInput.value = width;
  heightInput.value = height;
  clearError();
}

function updateDimension(changed) {
  if (!aspectLock.checked || !naturalWidth || !naturalHeight) return;
  const sourceRatio = naturalWidth / naturalHeight;
  if (changed === "width" && widthInput.value) {
    heightInput.value = Math.max(1, Math.round(Number(widthInput.value) / sourceRatio));
  } else if (changed === "height" && heightInput.value) {
    widthInput.value = Math.max(1, Math.round(Number(heightInput.value) * sourceRatio));
  }
}

async function resizeAndDownload() {
  clearError();
  const width = Number(widthInput.value);
  const height = Number(heightInput.value);
  if (!selectedFile || !Number.isInteger(width) || !Number.isInteger(height) ||
      width < 1 || height < 1 || width > 16384 || height > 16384) {
    showError("Enter a width and height between 1 and 16,384 pixels.");
    return;
  }

  resizeButton.disabled = true;
  progressWrap.hidden = false;
  setProgress(0, mediaType === "video" ? "Preparing video…" : "Resizing image…");
  try {
    if (mediaType === "image") {
      const blob = await resizeImage(width, height);
      downloadBlob(blob, outputName(document.querySelector("#format-select").value));
      setProgress(100, "Your image is ready");
    } else {
      const blob = await resizeVideo(width, height);
      downloadBlob(blob, outputName("video/webm"));
      setProgress(100, "Your video is ready");
    }
  } catch (error) {
    showError(error instanceof Error ? error.message : "The file could not be resized.");
    progressWrap.hidden = true;
  } finally {
    resizeButton.disabled = false;
  }
}

function resizeImage(width, height) {
  return new Promise((resolve, reject) => {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d");
    if (!context) {
      reject(new Error("Your browser could not create an image canvas."));
      return;
    }
    context.drawImage(imagePreview, 0, 0, width, height);
    const format = document.querySelector("#format-select").value;
    const quality = Number(document.querySelector("#quality-input").value) / 100;
    canvas.toBlob((blob) => {
      if (blob) resolve(blob);
      else reject(new Error("The image could not be exported in this format."));
    }, format, quality);
  });
}

async function resizeVideo(width, height) {
  if (!HTMLCanvasElement.prototype.captureStream || !window.MediaRecorder) {
    throw new Error("Video resizing is not supported by this browser. Try a recent version of Chrome, Edge, or Firefox.");
  }
  const mimeType = ["video/webm;codecs=vp9,opus", "video/webm;codecs=vp8,opus", "video/webm"]
    .find((type) => MediaRecorder.isTypeSupported(type));
  if (!mimeType) throw new Error("This browser cannot export WebM video.");

  const source = document.createElement("video");
  source.playsInline = true;
  source.preload = "auto";
  source.src = previewUrl;
  await waitForMetadata(source);
  source.currentTime = 0;
  await waitForSeek(source);

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Your browser could not create a video canvas.");
  context.drawImage(source, 0, 0, width, height);
  const outputStream = canvas.captureStream(30);
  let sourceStream;
  const captureStream = source.captureStream || source.mozCaptureStream;
  if (captureStream) {
    sourceStream = captureStream.call(source);
    for (const track of sourceStream.getAudioTracks()) outputStream.addTrack(track);
  }

  const recorder = new MediaRecorder(outputStream, { mimeType });
  const chunks = [];
  recorder.ondataavailable = (event) => {
    if (event.data.size) chunks.push(event.data);
  };
  const recording = new Promise((resolve, reject) => {
    recorder.onerror = () => reject(new Error("The video recording failed."));
    recorder.onstop = () => resolve(new Blob(chunks, { type: mimeType }));
  });
  recorder.start(250);
  setProgress(0, "Resizing video…");

  try {
    await source.play();
    await new Promise((resolve, reject) => {
      const drawFrame = () => {
        if (source.ended) {
          if (recorder.state !== "inactive") recorder.stop();
          resolve();
          return;
        }
        context.drawImage(source, 0, 0, width, height);
        if (Number.isFinite(source.duration) && source.duration > 0) {
          const percent = Math.min(99, Math.floor((source.currentTime / source.duration) * 100));
          setProgress(percent, "Resizing video…");
        }
        requestAnimationFrame(drawFrame);
      };
      source.addEventListener("ended", () => {
        if (recorder.state !== "inactive") recorder.stop();
      }, { once: true });
      drawFrame();
      source.addEventListener("error", () => reject(new Error("The video could not be decoded.")), { once: true });
    });
    const blob = await recording;
    if (blob.size === 0) throw new Error("The resized video was empty. Try another video format.");
    return blob;
  } catch (error) {
    if (recorder.state !== "inactive") recorder.stop();
    throw error;
  } finally {
    for (const track of outputStream.getTracks()) track.stop();
    if (sourceStream) for (const track of sourceStream.getTracks()) track.stop();
    source.pause();
    source.removeAttribute("src");
    source.load();
  }
}

function waitForMetadata(video) {
  if (video.readyState >= 1) return Promise.resolve();
  return new Promise((resolve, reject) => {
    video.addEventListener("loadedmetadata", resolve, { once: true });
    video.addEventListener("error", () => reject(new Error("This video could not be opened for resizing.")), { once: true });
  });
}

function waitForSeek(video) {
  if (video.currentTime === 0) return Promise.resolve();
  return new Promise((resolve) => video.addEventListener("seeked", resolve, { once: true }));
}

function outputName(mimeType) {
  const extension = mimeType === "image/jpeg" ? "jpg" : mimeType === "image/png" ? "png" :
    mimeType === "image/webp" ? "webp" : "webm";
  const baseName = selectedFile.name.replace(/\.[^.]+$/, "") || "resized";
  return `${baseName}-resized.${extension}`;
}

function downloadBlob(blob, filename) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function setProgress(percent, message) {
  progressBar.style.width = `${percent}%`;
  statusText.textContent = message;
  progressValue.textContent = percent ? `${percent}%` : "";
}

function showError(message) {
  errorMessage.textContent = message;
  errorMessage.hidden = false;
}

function clearError() {
  errorMessage.textContent = "";
  errorMessage.hidden = true;
}

function releasePreviewUrl() {
  if (previewUrl) URL.revokeObjectURL(previewUrl);
  previewUrl = undefined;
  imagePreview.removeAttribute("src");
  videoPreview.pause();
  videoPreview.removeAttribute("src");
  videoPreview.load();
}

function formatBytes(bytes) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
