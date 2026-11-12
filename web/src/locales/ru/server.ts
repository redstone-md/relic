import type { Dict } from "@/lib/i18n";

export default {
  // Tabs of a server.
  "Game rules": "Правила игры",
  Worlds: "Миры",
  Catalogue: "Каталог",
  "How this works": "Как это работает",

  // Console.
  "The server is stopped": "Сервер остановлен",
  "Its output appears here once it starts.": "Вывод появится здесь после запуска.",
  "Recent crashes": "Последние падения",
  "No crash reports. The game has not crashed on this server.":
    "Отчётов о падениях нет. Игра на этом сервере не падала.",
  "{count} older ones in Files, crash-reports": [
    "Ещё {count} старый в «Файлах», папка crash-reports",
    "Ещё {count} старых в «Файлах», папка crash-reports",
    "Ещё {count} старых в «Файлах», папка crash-reports",
  ],
  "The console shows what the server prints and takes the same commands an operator types in chat, without the slash.":
    "Консоль показывает вывод сервера и принимает те же команды, что оператор пишет в чате, только без косой черты.",
  "The graphs cover the last three minutes and are measured by this page, so they start empty when you open it.":
    "Графики охватывают последние три минуты. Их снимает сама страница, поэтому при открытии они пустые.",

  // Players of one server.
  "Online now": "Сейчас в игре",
  "Nobody is playing. The lists below can still be edited, for players who have an account here.":
    "Никто не играет. Списки ниже можно править и так, для игроков с аккаунтом на этой панели.",
  Kick: "Выгнать",
  Whitelist: "Белый список",
  "Only these players can join.": "Зайти могут только эти игроки.",
  "Off: anyone with an account can join. The list is kept for when you switch it on.":
    "Выключен: зайти может любой, у кого есть аккаунт. Список сохраняется до включения.",
  On: "Вкл.",
  Off: "Выкл.",
  "Nobody is on the whitelist": "В белом списке никого нет",
  "Player name": "Ник игрока",
  "Take {name} off the whitelist": "Убрать {name} из белого списка",
  Operators: "Операторы",
  "May run commands in the game: change modes, teleport, ban and stop the server.":
    "Могут выполнять команды в игре: менять режим, телепортироваться, банить и останавливать сервер.",
  "Nobody is an operator": "Операторов нет",
  "Make operator": "Сделать оператором",
  "Level {level}": "Уровень {level}",
  "Take operator rights from {name}": "Снять права оператора с {name}",
  "Banned players": "Забаненные игроки",
  "Cannot join this server. Their account on the panel stays as it is.":
    "Не могут зайти на этот сервер. Их аккаунт на панели остаётся как был.",
  "Nobody is banned": "Никто не забанен",
  "Reason, shown to the player": "Причина, её увидит игрок",
  "Lift the ban of {name}": "Снять бан с {name}",
  "Banned addresses": "Забаненные адреса",
  "Nobody can join from these IP addresses, whatever account they use.":
    "С этих IP-адресов не зайти ни под каким аккаунтом.",
  "No address is banned": "Забаненных адресов нет",
  "IP address, e.g. 203.0.113.7": "IP-адрес, например 203.0.113.7",
  "“{name}” has no account here. Start the server to list players by name alone.":
    "У «{name}» нет аккаунта на этой панели. Запустите сервер, чтобы добавлять игроков просто по нику.",
  "These are the server's own lists. While it runs, every change here is sent as a console command, so it applies at once.":
    "Это списки самого сервера. Пока он работает, каждое изменение уходит командой в консоль и действует сразу.",
  "While it is stopped, the lists are written directly and take effect at the next start. That needs the player's account id, so only players registered on this panel can be added then.":
    "Пока сервер остановлен, списки пишутся в файлы и начинают действовать со следующего запуска. Для этого нужен id аккаунта, поэтому добавить можно только игроков, зарегистрированных на этой панели.",
  "A ban here closes one server. To close every server and the launcher, ban the account on the panel's Players page.":
    "Бан здесь закрывает один сервер. Чтобы закрыть все серверы и лаунчер, забаньте аккаунт на странице «Игроки» панели.",

  // Game rules: the page.
  "Message of the day": "Описание сервера",
  "The two lines players see under the server's name in their server list.":
    "Две строки, которые игроки видят под названием сервера в списке серверов.",
  Colour: "Цвет",
  "Reset style": "Сбросить стиль",
  "Server icon": "Иконка сервера",
  "Shown beside the server in the list. Any picture works: it is cropped to a square and scaled to 64 by 64.":
    "Показывается рядом с сервером в списке. Подойдёт любая картинка: она обрезается до квадрата и сжимается до 64 на 64.",
  "Current icon": "Текущая иконка",
  "Choose a picture": "Выбрать картинку",
  Replace: "Заменить",
  "Icon saved. Players see it after the next start.":
    "Иконка сохранена. Игроки увидят её после следующего запуска.",
  "Search settings": "Поиск по настройкам",
  All: "Все",
  "No setting matches “{query}”": "По запросу «{query}» настроек нет",
  "The server has not started yet, so it has no settings file. What you save here is what it starts with.":
    "Сервер ещё не запускался, файла настроек у него нет. Что сохраните здесь, с тем он и стартует.",
  "A whole number from {min} to {max}": "Целое число от {min} до {max}",
  "Default: empty": "По умолчанию: пусто",
  "Default: {value}": "По умолчанию: {value}",
  on: "вкл.",
  off: "выкл.",
  "No unsaved changes": "Несохранённых изменений нет",
  "{count} unsaved changes": [
    "{count} несохранённое изменение",
    "{count} несохранённых изменения",
    "{count} несохранённых изменений",
  ],
  "Fix the marked values first": "Сначала исправьте отмеченные значения",
  Discard: "Отменить",
  "Save and restart": "Сохранить и перезапустить",
  "Saved. The server reads it at the next start.":
    "Сохранено. Сервер прочитает настройки при следующем запуске.",
  "These are the settings of server.properties. The server reads the file only when it starts, so a change made while it runs waits for a restart.":
    "Это настройки из server.properties. Сервер читает файл только при запуске, поэтому изменение на работающем сервере ждёт перезапуска.",
  "Only the settings you change are rewritten; everything else in the file stays as it was. Settings that mods add appear under Other.":
    "Переписываются только изменённые настройки, остальное в файле остаётся как было. Настройки, добавленные модами, лежат в разделе «Другое».",

  // Game rules: colours and styles of the message of the day.
  Black: "Чёрный",
  "Dark blue": "Тёмно-синий",
  "Dark green": "Тёмно-зелёный",
  "Dark aqua": "Тёмно-бирюзовый",
  "Dark red": "Тёмно-красный",
  Purple: "Фиолетовый",
  Gold: "Золотой",
  Grey: "Серый",
  "Dark grey": "Тёмно-серый",
  Blue: "Синий",
  Green: "Зелёный",
  Aqua: "Бирюзовый",
  Red: "Красный",
  Pink: "Розовый",
  Yellow: "Жёлтый",
  White: "Белый",
  Bold: "Жирный",
  Italic: "Курсив",
  Underline: "Подчёркнутый",
  Strikethrough: "Зачёркнутый",

  // Game rules: groups.
  Gameplay: "Игра",
  World: "Мир",
  Network: "Сеть",
  Performance: "Производительность",

  // Game rules: settings, as `PROPERTIES` lists them.
  "Game mode": "Режим игры",
  "The mode new players start in.": "Режим, в котором начинают новые игроки.",
  Survival: "Выживание",
  Creative: "Творческий",
  Adventure: "Приключение",
  Spectator: "Наблюдатель",
  "Force the game mode": "Принудительный режим игры",
  "Puts every player back into the default mode each time they join.":
    "Возвращает каждого игрока в режим по умолчанию при каждом входе.",
  Difficulty: "Сложность",
  "How hard mobs hit and whether hunger can kill.":
    "Насколько сильно бьют мобы и может ли голод убить.",
  Peaceful: "Мирная",
  Easy: "Лёгкая",
  Normal: "Нормальная",
  Hard: "Сложная",
  Hardcore: "Хардкор",
  "Players who die become spectators for good. Difficulty is locked to hard.":
    "Погибший игрок навсегда становится наблюдателем. Сложность закреплена на «Сложной».",
  "Player versus player": "Бой между игроками",
  "Lets players hurt each other.": "Игроки могут наносить урон друг другу.",
  "Allow flight": "Разрешить полёт",
  "Stops the server from kicking survival players who stay in the air. Mods with jetpacks or wings need it.":
    "Сервер перестаёт выгонять игроков в выживании, которые висят в воздухе. Нужно модам с джетпаками и крыльями.",
  "Command blocks": "Командные блоки",
  "Lets command blocks run their commands.": "Командные блоки выполняют свои команды.",
  "Spawn protection": "Защита спавна",
  "Blocks around the spawn point that only operators can change. 0 switches it off.":
    "Сколько блоков вокруг точки спавна могут менять только операторы. 0 выключает защиту.",
  "The Nether": "Незер",
  "Lets players travel through Nether portals.": "Игроки могут проходить через порталы в Незер.",
  Monsters: "Монстры",
  "Spawns hostile mobs at night and in the dark.": "Враждебные мобы появляются ночью и в темноте.",
  Animals: "Животные",
  "Spawns cows, sheep and other animals. Older versions only.":
    "Появляются коровы, овцы и другие животные. Только в старых версиях.",
  Villagers: "Жители",
  "Spawns villagers. Older versions only.": "Появляются жители. Только в старых версиях.",

  "World folder": "Папка мира",
  "The folder the world is kept in. Changing it makes the server open or create another world.":
    "Папка, где лежит мир. Если её сменить, сервер откроет или создаст другой мир.",
  Seed: "Сид",
  "Decides how a new world is generated. An existing world ignores it.":
    "Определяет, каким сгенерируется новый мир. На готовый мир не влияет.",
  "World type": "Тип мира",
  "The generator for a new world. Mods may add their own types.":
    "Генератор нового мира. Моды могут добавлять свои типы.",
  Structures: "Структуры",
  "Generates villages, strongholds and other structures in new chunks.":
    "В новых чанках генерируются деревни, крепости и другие структуры.",
  "World border limit": "Предел границы мира",
  "The farthest the world border may reach from the centre, in blocks.":
    "Как далеко от центра может отойти граница мира, в блоках.",

  "Player slots": "Мест для игроков",
  "How many players may be online at once.": "Сколько игроков может быть в игре одновременно.",
  "Only players on the whitelist may join. The list is on the Players page.":
    "Зайти могут только игроки из белого списка. Сам список на странице «Игроки».",
  "Enforce the whitelist": "Строгий белый список",
  "Kicks players who are online when they are taken off the whitelist.":
    "Выгоняет игрока из игры, как только его убрали из белого списка.",
  "Idle timeout": "Тайм-аут бездействия",
  "Minutes without input before a player is kicked. 0 never kicks.":
    "Через сколько минут бездействия игрока выгоняет. 0 не выгоняет никогда.",
  "Operator level": "Уровень оператора",
  "What operators may do: 1 only bypasses spawn protection, 4 may stop the server.":
    "Что можно операторам: 1 только обходит защиту спавна, 4 может остановить сервер.",
  "Hide the player list": "Скрыть список игроков",
  "Leaves the names of online players out of the server list.":
    "Ники игроков в сети не показываются в списке серверов.",
  "Account check": "Проверка аккаунтов",
  "Verifies every player's account. Relic keeps this on and checks accounts itself.":
    "Проверяет аккаунт каждого игрока. Relic держит это включённым и проверяет аккаунты сам.",
  "Set by Relic at every start": "Relic выставляет при каждом запуске",

  "The port players connect to.": "Порт, к которому подключаются игроки.",
  "Change it on the Settings page": "Меняется на странице «Настройки»",
  "Bind address": "Адрес привязки",
  "The network address the server listens on. Leave empty to listen on all of them.":
    "Сетевой адрес, на котором слушает сервер. Пусто значит на всех.",
  "Show in the server list": "Показывать в списке серверов",
  "Answers pings from the multiplayer menu. Off, the server looks offline but still accepts players.":
    "Отвечает на опрос из меню сетевой игры. Если выключить, сервер выглядит выключенным, но игроков пускает.",
  Query: "Query",
  "Answers the GameSpy4 query protocol that some monitoring sites use.":
    "Отвечает по протоколу GameSpy4, им пользуются некоторые мониторинги.",
  "Remote console": "Удалённая консоль",
  "Opens RCON for remote commands. The console on this panel does not need it.":
    "Открывает RCON для удалённых команд. Консоли этой панели он не нужен.",
  "Resource pack link": "Ссылка на ресурспак",
  "A direct link to a resource pack the server offers to every player.":
    "Прямая ссылка на ресурспак, который сервер предлагает каждому игроку.",
  "Require the resource pack": "Требовать ресурспак",
  "Disconnects players who decline the pack.": "Отключает игроков, отказавшихся от ресурспака.",

  "View distance": "Дальность прорисовки",
  "How many chunks around each player the server sends. The biggest lever on load.":
    "Сколько чанков вокруг игрока отправляет сервер. Сильнее всего влияет на нагрузку.",
  "Simulation distance": "Дальность симуляции",
  "How many chunks around each player mobs, crops and machines keep running in.":
    "В скольких чанках вокруг игрока продолжают работать мобы, посевы и механизмы.",
  "Watchdog limit": "Предел сторожа",
  "Milliseconds one tick may take before the server stops itself as hung. -1 switches the watchdog off.":
    "Сколько миллисекунд может длиться один тик, прежде чем сервер остановит себя как зависший. -1 выключает сторожа.",
  "Compression threshold": "Порог сжатия",
  "Packets at least this many bytes are compressed. -1 never compresses.":
    "Пакеты от такого размера в байтах сжимаются. -1 не сжимает никогда.",
  "Entity range": "Дальность сущностей",
  "How far away players still see mobs and items, as a percentage of the normal range.":
    "С какого расстояния игроки ещё видят мобов и предметы, в процентах от обычного.",
  "Synchronous chunk writes": "Синхронная запись чанков",
  "Waits for the disk on every chunk save. Safer after a power cut, slower on hard drives.":
    "Ждёт диск при каждом сохранении чанка. Надёжнее при отключении питания, медленнее на жёстких дисках.",

  // Worlds.
  Overworld: "Верхний мир",
  "The End": "Энд",
  "Current world": "Текущий мир",
  "Changed {date}": "Изменён {date}",
  "No world yet": "Мира пока нет",
  "The server generates one at its first start. To play an existing world, upload it as a zip.":
    "Сервер создаст его при первом запуске. Чтобы играть в готовом мире, загрузите его архивом zip.",
  "Dimensions that mods add, and the Nether and the End on servers without plugins, live inside the main folder.":
    "Измерения из модов, а на серверах без плагинов ещё Незер и Энд, лежат внутри основной папки.",
  "Uploading {name}: {sent} of {size}": "Загрузка {name}: {sent} из {size}",
  "Change the world": "Сменить мир",
  "Stop the server first: it keeps the world open while it runs.":
    "Сначала остановите сервер: пока он работает, мир открыт.",
  "Upload a world": "Загрузить мир",
  "A zip of a world folder, the one that holds level.dat.":
    "Архив zip с папкой мира, той, где лежит level.dat.",
  "Generate a new world": "Создать новый мир",
  "Puts the current world aside and lets the server build a fresh one.":
    "Убирает текущий мир в сторону, и сервер строит новый.",
  "Replace the world “{name}”?": "Заменить мир «{name}»?",
  "The current world moves to the trash in Files, from where it can be put back. “{file}” takes its place.":
    "Текущий мир уедет в корзину в «Файлах», оттуда его можно вернуть. На его место встанет «{file}».",
  "Replace the world": "Заменить мир",
  "Generate a new world?": "Создать новый мир?",
  "The current world moves to the trash in Files, from where it can be put back. Players start over in a new one at the next start.":
    "Текущий мир уедет в корзину в «Файлах», оттуда его можно вернуть. После следующего запуска игроки начнут в новом.",
  "Leave empty for a random world.": "Оставьте пустым для случайного мира.",
  "World uploaded. The server opens it at the next start.":
    "Мир загружен. Сервер откроет его при следующем запуске.",
  "Done. A new world is generated at the next start.":
    "Готово. Новый мир создастся при следующем запуске.",
  "The server plays the world in the folder named by “World folder” on the Game rules page.":
    "Сервер играет мир из папки, указанной в «Папка мира» на странице «Правила игры».",
  "Nothing here deletes a world. Replaced and regenerated worlds wait in the":
    "Здесь мир не удаляется. Заменённые и пересозданные миры лежат в",
  "trash in Files": "корзине в «Файлах»",
  ", and take disk space until you empty it.": " и занимают место на диске, пока её не очистить.",
  "For a copy you can go back to on a schedule, use Backups.":
    "Для копий по расписанию, к которым можно откатиться, есть «Резервные копии».",

  // Files.
  Storage: "Место на диске",
  "Files in the top folder": "Файлы в корне",
  "Everything else": "Всё остальное",
  Trash: "Корзина",
  "Empty the trash": "Очистить корзину",
  "Back to the files": "Назад к файлам",
  "Search this folder": "Поиск в этой папке",
  "{count} selected": ["Выбран {count}", "Выбрано {count}", "Выбрано {count}"],
  "Move to the trash": "В корзину",
  "Move {name} to the trash": "Переместить {name} в корзину",
  "Put back": "Вернуть",
  "Delete for good": "Удалить навсегда",
  "Delete {name} for good": "Удалить {name} навсегда",
  Clear: "Снять выбор",
  "Select everything shown": "Выбрать всё показанное",
  "Select {name}": "Выбрать {name}",
  "Was at": "Лежал в",
  "Thrown away": "Удалён",
  "Download as a zip": "Скачать архивом zip",
  "Download {name} as a zip": "Скачать {name} архивом zip",
  "Nothing here matches “{query}”": "По запросу «{query}» здесь ничего нет",
  "The search looks at the open folder only.": "Поиск идёт только по открытой папке.",
  "The trash is empty": "Корзина пуста",
  "Files you delete wait here until you empty it.":
    "Удалённые файлы лежат здесь, пока вы её не очистите.",
  "Everything here still takes disk space.": "Всё, что здесь лежит, по-прежнему занимает место на диске.",
  "Drop files anywhere on this tile to upload them. Deleted files wait in the trash.":
    "Перетащите файлы в любое место этой плитки, чтобы загрузить их. Удалённые файлы попадают в корзину.",
  "Delete {count} items for good?": [
    "Удалить {count} элемент навсегда?",
    "Удалить {count} элемента навсегда?",
    "Удалить {count} элементов навсегда?",
  ],
  "They leave the trash and the disk. This cannot be undone.":
    "Они исчезнут из корзины и с диска. Это нельзя отменить.",
  "{count} items deleted": [
    "Удалён {count} элемент",
    "Удалено {count} элемента",
    "Удалено {count} элементов",
  ],
  "{count} items moved to the trash": [
    "{count} элемент перемещён в корзину",
    "{count} элемента перемещено в корзину",
    "{count} элементов перемещено в корзину",
  ],
  "{count} items put back": [
    "{count} элемент возвращён",
    "{count} элемента возвращено",
    "{count} элементов возвращено",
  ],
  "{name} does not say where it came from. Rename it in place.":
    "По имени {name} не понять, откуда он. Переименуйте его прямо здесь.",
  "This is the server's own folder. Click a text file to edit it; anything else downloads, and a folder downloads as a zip.":
    "Это папка самого сервера. Текстовый файл открывается для правки по клику, остальное скачивается, а папка скачивается архивом zip.",
  "Deleting moves a file to the trash, from where it can be put back to where it was. Files marked “modpack” come back with the next deploy: remove those on the Modpack page.":
    "Удаление переносит файл в корзину, оттуда его можно вернуть на место. Файлы с пометкой «сборка» вернутся при следующем развёртывании: их убирают на странице «Сборка».",

  // Catalogue.
  Modpacks: "Сборки",
  "Single mods, added to the modpack one at a time.": "Отдельные моды, добавляются в сборку по одному.",
  "Whole modpacks, ready to play.": "Готовые сборки целиком.",
  "Search modpacks": "Поиск сборок",
  "Search modpacks for {loader} {version}": "Поиск сборок для {loader} {version}",
  "Kind of modpack": "Вид сборки",
  "Any kind": "Любой вид",
  Order: "Порядок",
  "Adventure ## kind of modpack": "Приключения",
  Challenging: "Сложные",
  Combat: "Бои",
  "Kitchen sink": "Всё подряд",
  Lightweight: "Лёгкие",
  Magic: "Магия",
  Multiplayer: "Для сетевой игры",
  Optimization: "Оптимизация",
  Quests: "Квесты",
  Technology: "Техника",
  "Best match": "По совпадению",
  "Most downloaded": "По скачиваниям",
  "Most followed": "По подписчикам",
  "Recently updated": "Недавно обновлённые",
  Install: "Установить",
  "Install {title}?": "Установить {title}?",
  "Its newest build for {loader} {version} is downloaded from Modrinth and its mods and configs are added to this server's modpack. Files already there with the same name are replaced; nothing else is removed.":
    "С Modrinth скачается её последняя версия для {loader} {version}, а моды и конфиги добавятся в сборку этого сервера. Файлы с теми же именами заменятся, остальное останется на месте.",
  "Downloading {title} and its mods. A large pack takes a few minutes.":
    "Скачивается {title} и её моды. Большая сборка занимает несколько минут.",
  "No modpack for {loader} {version} matches. Try another kind or fewer words.":
    "Сборок для {loader} {version} по такому запросу нет. Попробуйте другой вид или меньше слов.",
  "Vanilla Minecraft loads no modpacks": "Ванильный Minecraft сборки не загружает",
  "Pick a loader in Settings first; the catalogue then shows the packs made for it.":
    "Сначала выберите загрузчик в «Настройках», и каталог покажет сборки под него.",
  "Everything added here goes into the modpack: the server gets it at the next deploy and players' launchers download it before the next game.":
    "Всё добавленное здесь попадает в сборку: сервер получит это при следующем развёртывании, а лаунчеры игроков скачают перед следующей игрой.",
  "A mod that needs a library says so when you add it, with a button that adds the library too. Check the result on the Modpack page.":
    "Если моду нужна библиотека, при добавлении появится сообщение с кнопкой, которая добавит и её. Результат смотрите на странице «Сборка».",

  // Modpack rows.
  "Needs {list}": "Требует: {list}",
  "Not in the modpack: {list}. The game will not start without it.":
    "Нет в сборке: {list}. Без этого игра не запустится.",

  // Sync.
  Sync: "Синхронизация",
  "What the launcher keeps identical to the modpack, and what stays the player's own.":
    "Что лаунчер держит точно как в сборке, а что остаётся личным у игрока.",
  "Mods players may run": "Какие моды можно игрокам",
  "Folders besides mods": "Другие папки",
  "Only the modpack's resource packs": "Только ресурспаки из сборки",
  "The launcher removes any other pack from resourcepacks before each game. Keeps out see-through texture packs.":
    "Перед каждой игрой лаунчер убирает из resourcepacks всё чужое. Так не пройдут ресурспаки с прозрачными блоками.",
  "Only the modpack's shaders": "Только шейдеры из сборки",
  "The launcher removes any other shader pack from shaderpacks before each game.":
    "Перед каждой игрой лаунчер убирает из shaderpacks все чужие шейдеры.",
  "Always the player's own": "Всегда личное у игрока",
  "Game settings, key binds and the server list arrive with the first install and are never overwritten after that. Configs marked “Copy once” on the Modpack page update only while the player has not edited them.":
    "Настройки игры, управление и список серверов приходят при первой установке и больше не перезаписываются. Конфиги с пометкой «Скопировать один раз» на странице «Сборка» обновляются, только пока игрок их не правил.",
  "These checks stop casual cheating. A player with a modified launcher can get past them, so keep an anti-cheat mod on the server for anything that matters.":
    "Эти проверки останавливают только простое читерство. Игрок с переделанным лаунчером их обойдёт, поэтому для серьёзной защиты держите на сервере античит-мод.",
} satisfies Dict;
