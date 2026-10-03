/** Every user-visible string of the UI, in one place. The app is English-only. */
const messages = {
  app: {
    name: "FileForge",
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
    dismiss: "Dismiss",
  },
  errors: {
    notAFile: "This is a folder or another non-regular file.",
    io: "The file could not be read or written.",
    internal: "Something went wrong inside FileForge. Please try again.",
    unknown: "Unexpected error.",
  },
  footer: {
    credit: "Developed by Chepio",
  },
};

export type Messages = typeof messages;

export default messages;
