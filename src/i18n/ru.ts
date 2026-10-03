// Types
import type { Messages } from "./en";
// Utils
import { plural } from "./plural";

const ru: Messages = {
  app: {
    name: "FileForge",
  },
  nav: {
    label: "Инструменты",
    soon: "Скоро",
    groups: {
      documents: "Документы",
      images: "Изображения",
      video: "Видео",
      audio: "Аудио",
    },
    language: "Язык интерфейса",
  },
  tools: {
    pdfCompress: {
      nav: "Сжать",
      title: "Сжать PDF",
      description: "Перестраивает структуру PDF, не трогая содержимое, и по желанию пережимает изображения.",
    },
    pdfConvert: { nav: "Конвертировать", title: "Конвертировать PDF" },
    imageCompress: { nav: "Сжать", title: "Сжать изображения" },
    imageConvert: { nav: "Конвертировать", title: "Конвертировать изображения" },
    videoCompress: { nav: "Сжать", title: "Сжать видео" },
    videoConvert: { nav: "Конвертировать", title: "Конвертировать видео" },
    audioConvert: { nav: "Конвертировать", title: "Конвертировать аудио" },
  },
  intake: {
    dropTitle: (formats) => `Перетащите сюда файлы ${formats}`,
    or: "или",
    choose: "Выбрать файлы…",
    addMore: "Добавить файлы…",
    releaseToAdd: "Отпустите, чтобы добавить файлы",
    privacy: "Файлы не покидают компьютер. Оригиналы не изменяются.",
    filterName: (formats) => `Файлы ${formats}`,
    filesHeading: (count) => `${count} ${plural(count, { one: "файл", few: "файла", many: "файлов" })}`,
    totalSize: (size) => `всего ${size}`,
    remove: (name) => `Убрать ${name}`,
    clear: "Очистить список",
    skipped: (names) => `Пропущено (не файл): ${names}`,
    wrongKind: (formats, names) => `Здесь принимаются только файлы ${formats}. Пропущено: ${names}`,
    dismiss: "Закрыть",
  },
  errors: {
    notAFile: "Это папка или другой нестандартный файл.",
    io: "Не удалось прочитать или записать файл.",
    internal: "Внутренняя ошибка FileForge. Попробуйте ещё раз.",
    unknown: "Непредвиденная ошибка.",
  },
  footer: {
    credit: "Разработано Chepio",
  },
};

export default ru;
