import type { Dict } from "@/lib/i18n";

export default {
  // Sides, as `SIDE_NAMES` lists them.
  Client: "Клиент",
  Server: "Сервер",
  Both: "Обе",
  Unknown: "Неизвестно",

  // Modpack files.
  Mods: "Моды",
  Configs: "Конфиги",
  "Resource packs": "Ресурспаки",
  "Shader packs": "Шейдерпаки",
  Other: "Другое",
  "By file type": "По типу файла",
  "Instance root": "Корень сервера",
  "Modpack archives are imported on the Add mods tab.":
    "Архивы сборок импортируются на вкладке «Моды».",
  "{count} files uploaded": [
    "Загружен {count} файл",
    "Загружено {count} файла",
    "Загружено {count} файлов",
    "Загружено {count} файла",
  ],
  "Modpack files": "Файлы сборки",
  "{count} files, {size}": [
    "{count} файл, {size}",
    "{count} файла, {size}",
    "{count} файлов, {size}",
    "{count} файла, {size}",
  ],
  build: "версия",
  "Upload destination": "Куда загружать",
  Upload: "Загрузить",
  "{count} files have no side yet.": [
    "У {count} файла не выбрана сторона.",
    "У {count} файлов не выбрана сторона.",
    "У {count} файлов не выбрана сторона.",
    "У {count} файла не выбрана сторона.",
  ],
  "Until you pick one, they reach neither the server nor players. Show them":
    "Пока сторона не выбрана, такие файлы не попадают ни на сервер, ни к игрокам. Показать",
  Waiting: "Ожидание",
  "{sent} of {size}": "{sent} из {size}",
  Dismiss: "Скрыть",
  "Filter by name or path": "Фильтр по названию или пути",
  "Filter files": "Фильтр файлов",
  Kind: "Тип",
  "All kinds": "Все типы",
  Side: "Сторона",
  "All sides": "Все стороны",
  "Drop mods, configs or whole folders here": "Перетащите сюда моды, конфиги или целые папки",
  "Jars go to mods/, archives to resourcepacks/, config files to config/. A dropped folder keeps its structure.":
    "Файлы .jar попадают в mods/, архивы — в resourcepacks/, конфиги — в config/. Структура перетащенной папки сохраняется.",
  "Nothing matches the filter.": "По фильтру ничего не найдено.",
  File: "Файл",
  Size: "Размер",
  Updates: "Обновления",
  "Optional ## file column": "Необязательный",
  "Side of {name}": "Сторона файла {name}",
  Classify: "Выбрать",
  "Detected: {side}": "Определено: {side}",
  "Set by hand": "Задано вручную",
  "Declared by the mod": "Указано в моде",
  "Best guess": "Предположение",
  "Not detected": "Не определено",
  Default: "По умолчанию",
  "Updates of {name}": "Обновления файла {name}",
  "Always match": "Всегда обновлять",
  "Copy once": "Скопировать один раз",
  "{name} is optional": "{name}: необязательный файл",
  "Delete {name}": "Удалить {name}",
  "“Copy once” files reach a player on first install and are never overwritten after that, which suits configs players tune themselves. Optional files can be switched off in the launcher.":
    "Файлы с режимом «Скопировать один раз» попадают к игроку при первой установке и больше не перезаписываются — это удобно для конфигов, которые игроки настраивают сами. Необязательные файлы можно отключить в лаунчере.",
  "Drop to upload": "Отпустите, чтобы загрузить",
  "Remove this file?": "Удалить этот файл?",
  "{path} leaves the modpack. Launchers delete it from players' installs on the next sync.":
    "Файл {path} будет убран из сборки. При следующей синхронизации лаунчеры удалят его у игроков.",
  Remove: "Удалить",

  // Adding mods.
  "Builds for {loader} {version}, newest first.": "Версии для {loader} {version}, сначала новые.",
  "No build of this mod fits the instance.": "Для этого сервера нет подходящей версии мода.",
  "{count} files added to the modpack.": [
    "В сборку добавлен {count} файл.",
    "В сборку добавлено {count} файла.",
    "В сборку добавлено {count} файлов.",
    "В сборку добавлено {count} файла.",
  ],
  "{count} files added to the modpack, {failed} failed.": [
    "В сборку добавлен {count} файл, не удалось добавить: {failed}.",
    "В сборку добавлено {count} файла, не удалось добавить: {failed}.",
    "В сборку добавлено {count} файлов, не удалось добавить: {failed}.",
    "В сборку добавлено {count} файла, не удалось добавить: {failed}.",
  ],
  "The pack targets {target}, which differs from this instance. Change the platform in Settings or expect mods to fail to load.":
    "Сборка рассчитана на {target}, а у этого сервера платформа другая. Смените платформу на вкладке «Настройки», иначе моды могут не загрузиться.",
  "another version": "другую версию",
  "{title} added": "Мод {title} добавлен",
  "Needs {mods}": "Требуется: {mods}",
  "Add it": "Добавить",
  "Add all {count}": "Добавить все ({count})",
  "Import a modpack": "Импорт сборки",
  "A Modrinth .mrpack brings its mods, configs and the side of every file in one step.":
    "Файл .mrpack с Modrinth за один шаг добавляет моды, конфиги и сторону каждого файла.",
  Importing: "Импорт",
  "Choose .mrpack": "Выбрать .mrpack",
  "Search {provider} for {loader} {version}": "Поиск на {provider} для {loader} {version}",
  "Search mods": "Поиск модов",
  Search: "Найти",
  "This instance runs vanilla Minecraft, which loads no mods. Pick a loader in Settings first.":
    "На этом сервере обычный Minecraft без загрузчика, моды в нём не работают. Сначала выберите загрузчик на вкладке «Настройки».",
  "CurseForge needs an API key": "Для CurseForge нужен ключ API",
  "CurseForge only answers requests signed with your own key.":
    "CurseForge отвечает только на запросы с вашим собственным ключом.",
  "Add the key in Settings": "Добавить ключ в настройках",
  Searching: "Поиск",
  "Nothing found for “{term}” on this platform.":
    "По запросу «{term}» на этой платформе ничего не найдено.",
  "Nothing found on this platform.": "На этой платформе ничего не найдено.",
  "The author does not allow third-party downloads. Get the file from the page and upload it on the Files tab.":
    "Автор запретил скачивание через сторонние сервисы. Скачайте файл со страницы мода и загрузите его на вкладке «Файлы».",
  "Manual download": "Скачать вручную",
  Versions: "Версии",
  Added: "Добавлено",
  Add: "Добавить",

  // Backups.
  "Backup saved, {size}": "Резервная копия сохранена, {size}",
  "World restored from backup": "Мир восстановлен из резервной копии",
  "Backup deleted": "Резервная копия удалена",
  "World backups": "Резервные копии мира",
  "A running server pauses saving for the copy, so players are not interrupted. The schedule and retention live in Settings.":
    "Работающий сервер на время копирования приостанавливает сохранение, так что игрокам это не мешает. Расписание и срок хранения задаются в настройках.",
  "Backing up": "Создаём копию",
  "Back up now": "Создать копию",
  "No backups yet": "Резервных копий пока нет",
  "Take one before changing mods or updating the Minecraft version.":
    "Создайте копию перед сменой модов или обновлением версии Minecraft.",
  Taken: "Создана",
  Download: "Скачать",
  "Stop the server to restore": "Остановите сервер, чтобы восстановить",
  Restore: "Восстановить",
  "Delete backup": "Удалить резервную копию",
  "Stop the server to restore a backup.": "Чтобы восстановить резервную копию, остановите сервер.",
  "Restore this backup?": "Восстановить эту резервную копию?",
  "The current world is replaced with the copy from {date}. Progress made since then is lost.":
    "Текущий мир будет заменён копией от {date}. Всё, что сделано после этого, пропадёт.",
  "Delete this backup?": "Удалить эту резервную копию?",
  "The copy from {date} is removed from disk.": "Копия от {date} будет удалена с диска.",
  Delete: "Удалить",

  // Server settings.
  Strict: "Строгий",
  Optional: "Гибкий",
  "Players run exactly this modpack. Any extra jar in their mods folder is removed.":
    "Игроки играют ровно с этой сборкой. Лишние файлы .jar из папки mods удаляются.",
  "Players may switch the files you mark optional on or off. Nothing else is allowed.":
    "Игроки могут включать и отключать файлы, которые вы отметили необязательными. Больше ничего менять нельзя.",
  Permissive: "Свободный",
  "Players may add their own client mods. The launcher leaves unknown files alone.":
    "Игроки могут добавлять свои клиентские моды. Лаунчер не трогает неизвестные файлы.",
  "Settings saved": "Настройки сохранены",
  "{name} deleted": "Сервер {name} удалён",
  Identity: "Основное",
  Name: "Название",
  Description: "Описание",
  Id: "Идентификатор",
  Platform: "Платформа",
  "The server reinstalls on its next start, and players download the new client. Back up the world first: older versions cannot open a newer world.":
    "При следующем запуске сервер будет переустановлен, а игроки скачают новый клиент. Сначала сделайте резервную копию мира: старые версии не открывают мир, созданный в более новой.",
  Resources: "Ресурсы",
  Port: "Порт",
  "Memory, MB": "Память, МБ",
  "Java version": "Версия Java",
  "Port, memory and Java apply on the next start. The runtime is downloaded automatically.":
    "Порт, память и Java применяются при следующем запуске. Нужная версия Java скачивается автоматически.",
  "Start with the daemon": "Запускать вместе с Relic",
  "Bring this server up whenever Relic itself starts.":
    "Сервер будет запускаться при каждом запуске Relic.",
  "Restart after a crash": "Перезапускать после сбоя",
  "Retries with a growing delay, and gives up if crashes keep coming.":
    "Попытки идут с растущей задержкой и прекращаются, если сбои повторяются.",
  "Player mods": "Моды игроков",
  "Mod policy": "Политика модов",
  "Save changes": "Сохранить изменения",
  "Unsaved changes": "Есть несохранённые изменения",
  "Accepted for this instance.": "Принято для этого сервера.",
  "The server refuses to start until you accept the": "Сервер не запустится, пока вы не примете",
  "Accept the EULA": "Принять EULA",
  "Delete instance": "Удалить сервер",
  "Removes the instance from Relic and from every launcher.":
    "Сервер будет удалён из Relic и из всех лаунчеров.",
  "Stop the server first.": "Сначала остановите сервер.",
  "Delete {name}?": "Удалить {name}?",
  "Players lose access to this modpack at once. This cannot be undone.":
    "Игроки сразу потеряют доступ к этой сборке. Отменить это нельзя.",
  "Also delete the world, configs and backups from disk":
    "Также удалить с диска мир, конфиги и резервные копии",
  "Left unchecked, the server folder stays where it is.":
    "Если не отмечать, папка сервера останется на месте.",
} satisfies Dict;
