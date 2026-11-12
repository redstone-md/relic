import type { Dict } from "@/lib/i18n";

export default {
  // Sections of an instance.
  Console: "Консоль",
  Files: "Файлы",
  Modpack: "Сборка",
  "Add mods": "Добавить моды",
  Backups: "Резервные копии",
  "No such instance": "Такого сервера нет",
  "Nothing is registered under “{id}”.": "Под именем «{id}» ничего не зарегистрировано.",
  "Back to overview": "К обзору",
  "{name} sections": "Разделы {name}",

  // The file manager.
  Folder: "Папка",
  "New folder": "Новая папка",
  "New file": "Новый файл",
  Changed: "Изменён",
  "Up one folder": "На папку выше",
  Rename: "Переименовать",
  "Rename {name}": "Переименовать {name}",
  "Download {name}": "Скачать {name}",
  "Delete {name}": "Удалить {name}",
  "Delete {name}?": "Удалить {name}?",
  "Delete the folder {name}?": "Удалить папку {name}?",
  "Everything inside it is deleted too. This cannot be undone.":
    "Всё её содержимое тоже удалится. Отменить это нельзя.",
  "This cannot be undone. The next modpack deploy puts the file back.":
    "Отменить это нельзя. При следующей выкладке сборки файл вернётся.",
  "This cannot be undone.": "Отменить это нельзя.",
  "{name} saved": "{name} сохранён",
  "This name is taken": "Это имя занято",
  "The file could not be read": "Не удалось прочитать файл",
  "This is not a text file. Download it instead.": "Это не текстовый файл. Скачайте его.",
  "Too large or not text: download it": "Слишком большой или не текст: скачайте его",
  "Folders cannot be dropped. Upload their files instead.":
    "Папки перетащить нельзя. Загрузите файлы из них.",
  "Uploading {name}": "Загружается {name}",
  "Uploading {name}, {index} of {count}": "Загружается {name}, {index} из {count}",
  "This folder is empty": "Папка пуста",
  "Drop files here or press Upload.": "Перетащите сюда файлы или нажмите «Загрузить».",
  "Drop files anywhere on this tile to upload them into the open folder.":
    "Перетащите файлы в любое место этой плитки, и они загрузятся в открытую папку.",
  "The server is running. It reads most files only at start and may overwrite its own while it runs.":
    "Сервер запущен. Большинство файлов он читает только при старте, а свои может перезаписать во время работы.",
  modpack: "сборка",
  "Placed here from the modpack. The next deploy puts the modpack's version back.":
    "Файл из сборки. Следующая выкладка вернёт версию из сборки.",
  "In {folder}": "В {folder}",
  "Changes are not saved yet": "Изменения ещё не сохранены",
  Saved: "Сохранено",
  Close: "Закрыть",

  // Pictures and fonts for the launcher.
  Uploading: "Загрузка",
  "Remove {what}": "Убрать: {what}",
  "The file is larger than 16 MB": "Файл больше 16 МБ",
  Picture: "Картинка",
  "Optional. Upload one or paste an https address.":
    "Необязательно. Загрузите файл или вставьте адрес https.",
  "Square PNG or WebP. Shown in the launcher's corner and on its sign-in screen.":
    "Квадратный PNG или WebP. Виден в углу лаунчера и на экране входа.",
  "Wide PNG, JPEG, WebP or GIF.": "Широкий PNG, JPEG, WebP или GIF.",
  "TTF or OTF.": "TTF или OTF.",

  // How a modpack looks in the launcher.
  "Look in the launcher": "Вид в лаунчере",
  "What players see behind this modpack. Launchers pick the change up within a minute.":
    "Что игроки видят за этой сборкой. Лаунчеры подхватят изменение в течение минуты.",
  "Animated scene": "Анимированная сцена",
  "Drawn by the launcher itself. Shown when there is no banner.":
    "Её рисует сам лаунчер. Видна, пока нет баннера.",
  "Mine shaft": "Шахта",
  Deepslate: "Глубинный сланец",
  Forest: "Лес",
  Desert: "Пустыня",
  Snow: "Снег",
  Banner: "Баннер",
  "PNG, JPEG, WebP or an animated GIF, up to 16 MB. The launcher does not play video.":
    "PNG, JPEG, WebP или анимированный GIF, до 16 МБ. Видео лаунчер не показывает.",
} satisfies Dict;
