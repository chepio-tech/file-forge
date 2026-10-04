/** Every user-visible string of the UI, in one place. The app is English-only. */
const messages = {
  app: {
    name: "File Forge",
  },
  nav: {
    label: "Tools",
    soon: "Soon",
    groups: {
      documents: "Documents",
      images: "Images",
      video: "Video",
      audio: "Audio",
    },
  },
  /** `nav` is the short label under the group heading; `title` is the full name for the panel and screen readers. */
  tools: {
    pdfCompress: {
      nav: "Compress",
      title: "Compress PDF",
      description: "Rewrites the PDF structure without touching content, and optionally recompresses images.",
    },
    pdfConvert: { nav: "Convert", title: "Convert PDF" },
    imageCompress: { nav: "Compress", title: "Compress images" },
    imageConvert: { nav: "Convert", title: "Convert images" },
    videoCompress: { nav: "Compress", title: "Compress video" },
    videoConvert: { nav: "Convert", title: "Convert video" },
    audioConvert: { nav: "Convert", title: "Convert audio" },
  },
  intake: {
    dropTitle: (formats: string) => `Drop ${formats} files here`,
    or: "or",
    choose: "Choose files…",
    addMore: "Add files…",
    releaseToAdd: "Release to add files",
    privacy: "Files never leave this computer. Originals are not modified.",
    filterName: (formats: string) => `${formats} files`,
    filesHeading: (count: number) => (count === 1 ? "1 file" : `${count} files`),
    totalSize: (size: string) => `${size} total`,
    remove: (name: string) => `Remove ${name}`,
    clear: "Clear list",
    skipped: (names: string) => `Skipped (not a file): ${names}`,
    wrongKind: (formats: string, names: string) => `Only ${formats} files are accepted here. Skipped: ${names}`,
    /** Dropped folders are searched for the tool's formats (ADR-0017); `folders` is how many were dropped. */
    noneInFolders: (formats: string, folders: number) =>
      `No ${formats} files found in the dropped ${folders === 1 ? "folder" : "folders"}.`,
    ignoredInFolders: (count: number, folders: number) =>
      `${count} ${count === 1 ? "file of another type" : "files of other types"} in the dropped ${folders === 1 ? "folder" : "folders"} ${count === 1 ? "was" : "were"} not added.`,
    foldersTruncated: (folders: number) =>
      folders === 1
        ? "The dropped folder is too large or too deeply nested to add at once, so some files were not added. Drop smaller folders."
        : "The dropped folders are too large or too deeply nested to add at once, so some files were not added. Drop smaller folders.",
    dismiss: "Dismiss",
  },
  pdf: {
    settings: "Compression",
    presets: { lossless: "Lossless", balanced: "Balanced", maximum: "Maximum", screen: "Screen", custom: "Custom" },
    losslessHint: "Rewrites the file structure only. Text, vectors and image pixels stay bit-identical.",
    lossyHint: (quality: number, maxDpi: number | null) =>
      maxDpi === null
        ? `JPEG and JPEG 2000 images saved as JPEG at quality ${quality}. Image resolution is kept.`
        : `JPEG and JPEG 2000 images saved as JPEG at quality ${quality}. Images shown above ${maxDpi} DPI are downsampled.`,
    screenHint: "For reading on screen: printed or zoomed-in pages look softer.",
    stripMetadata: "Remove metadata and thumbnails",
    stripMetadataHint: "Author, software, dates, XMP and page thumbnails. PDF/A, PDF/UA and PDF/X keep what they require.",
    stripEditingData: "Remove editing data",
    stripEditingDataHint: "Private Illustrator and Photoshop data. Those apps can no longer edit the file natively.",
    jpegQuality: "JPEG quality",
    maxDpi: "Max DPI",
    noLimit: "Keep",
    outdated: "Settings changed since the last run. Compress again to apply them.",
    compress: (count: number) => (count === 1 ? "Compress 1 file" : `Compress ${count} files`),
    compressing: (current: number, total: number) => `Compressing ${current} of ${total}…`,
    cancel: "Cancel",
    cancelling: "Cancelling…",
    cancelled: "Cancelled",
    waiting: "Waiting",
    working: "Compressing…",
    stages: {
      loading: "Reading",
      structure: "Cleaning structure",
      images: "Images",
      streams: "Streams",
      saving: "Writing",
      verifying: "Verifying",
    },
    /** "Images 3 of 12" for counted stages, "Reading…" otherwise. */
    stageProgress: (stage: string, done: number, total: number) =>
      total > 0 ? `${stage} ${done} of ${total}` : `${stage}…`,
    alreadyOptimal: "Already optimal",
    alreadyOptimalNothingRemoved: "Already optimal: original kept, nothing removed",
    reduction: (percent: string) => `−${percent}%`,
    save: "Save…",
    saving: "Saving…",
    saveAll: (count: number) => (count === 1 ? "Save 1 result to folder…" : `Save ${count} results to folder…`),
    saved: "Saved",
    savedAs: (name: string) => `Saved as ${name}. Click to show it in its folder.`,
    total: (before: string, after: string) => `${before} → ${after}`,
    removed: {
      metadata: "metadata removed",
      keptForStandard: "document metadata kept for PDF/A, PDF/UA or PDF/X",
      thumbnails: (count: number) => `${count} ${count === 1 ? "thumbnail" : "thumbnails"} removed`,
      editingData: "editing data removed",
    },
    details: (pages: number, recompressed: number, downsampled: number, duplicates: number) =>
      [
        `${pages} ${pages === 1 ? "page" : "pages"}`,
        `${recompressed} ${recompressed === 1 ? "image" : "images"} re-encoded, ${downsampled} downsampled`,
        `${duplicates} duplicate ${duplicates === 1 ? "stream" : "streams"} merged`,
      ].join(" · "),
  },
  updates: {
    check: "Check for updates",
    checking: "Checking for updates…",
    upToDate: (version: string) => `File Forge ${version} is up to date.`,
    available: (version: string) => `File Forge ${version} is available.`,
    install: "Restart to update",
    installing: "Downloading the update…",
    unsaved: "Compressed files you have not saved will be lost when File Forge restarts.",
    discard: "Update anyway",
    keep: "Cancel",
    checkFailed: "Could not check for updates. Check your internet connection.",
    retry: "Try again",
  },
  errors: {
    unknownFile: "This file is no longer in the list.",
    noResult: "Compress the file before saving it.",
    originalTarget: "Choose a different file name. Original files cannot be overwritten.",
    notAFile: "This is a folder or another non-regular file.",
    pdfTooLarge: "This PDF is larger than the 1 GB limit.",
    pdfEncrypted: "Password-protected PDF. File Forge never removes protection, so it was skipped.",
    pdfSigned: "Digitally signed PDF. Compressing would invalidate the signature, so it was skipped.",
    pdfMalformed: "This file is damaged or not a valid PDF.",
    invalidOptions: "These settings are out of range.",
    cancelled: "Compression was cancelled.",
    busy: "Wait until the current compression or save finishes, then try again.",
    unsavedResults: "Save your compressed files first, or choose to update anyway.",
    update: "The update could not be downloaded or installed. Nothing was changed.",
    io: "The file could not be read or written.",
    internal: "Something went wrong inside File Forge. Please try again.",
    unknown: "Unexpected error.",
  },
  footer: {
    credit: "Developed by Chepio",
  },
};

export type Messages = typeof messages;

export default messages;
