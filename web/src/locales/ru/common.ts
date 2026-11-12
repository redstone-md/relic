import type { Dict } from "@/lib/i18n";

// The frame around every page, the list of servers, the console and the
// words that pages share: statuses, power buttons, spans of time.
export default {
  // Top bar.
  Main: "Главное меню",
  Instances: "Серверы",
  Language: "Язык",
  "Light theme": "Светлая тема",
  "Dark theme": "Тёмная тема",
  "Sign out": "Выйти",
  Connecting: "Подключение",

  // Statuses of a server, as `STATUS_NAMES` lists them.
  Stopped: "Остановлен",
  Installing: "Установка",
  Starting: "Запуск",
  Running: "Работает",
  Stopping: "Остановка",
  Crashed: "Сбой",

  // Power.
  Start: "Запустить",
  Restart: "Перезапустить",
  Stop: "Остановить",
  "Accept the Minecraft EULA": "Принять EULA Minecraft",
  "A Minecraft server only starts once you agree to the":
    "Сервер Minecraft запустится только после того, как вы примете",
  "Minecraft End User License Agreement": "лицензионное соглашение Minecraft",
  "Accepting here writes eula=true for {name}.":
    "Принимая его здесь, вы записываете eula=true для сервера {name}.",
  "Accept and start": "Принять и запустить",

  // Spans of time.
  "{days}d {hours}h": "{days} д {hours} ч",
  "{hours}h {minutes}m": "{hours} ч {minutes} мин",
  "{minutes}m": "{minutes} мин",
  "{seconds}s": "{seconds} с",

  // List of servers.
  Address: "Адрес",
  Memory: "Память",
  Uptime: "Время работы",
  "Players connect to {host}": "Игроки подключаются к {host}",
  "Set a public address in Settings so launchers can find this server.":
    "Укажите публичный адрес в настройках, чтобы лаунчеры могли найти этот сервер.",
  "No instances yet": "Серверов пока нет",
  "An instance is one Minecraft server with its own modpack, world and port.":
    "Каждый сервер — это отдельный сервер Minecraft со своей сборкой, миром и портом.",
  "Create the first one": "Создать первый",
  Host: "Машина",
  Processor: "Процессор",
  "Processor load, last three minutes": "Загрузка процессора за последние три минуты",
  "Host memory in use": "Занятая память машины",
  online: "в сети",
  accounts: ["аккаунт", "аккаунта", "аккаунтов", "аккаунта"],

  // Console.
  Live: "На связи",
  Reconnecting: "Переподключение",
  "{count} lines": ["{count} строка", "{count} строки", "{count} строк", "{count} строки"],
  "Clear view": "Очистить экран",
  "{name} console": "Консоль сервера {name}",
  "The server is stopped. Output appears here once it starts.":
    "Сервер остановлен. Вывод появится здесь после запуска.",
  "Waiting for output.": "Ждём вывод.",
  "Jump to latest": "К последним строкам",
  "Server command": "Команда серверу",
  "Type a command, e.g. say hello": "Введите команду, например say hello",
  "Start the server to send commands": "Запустите сервер, чтобы отправлять команды",
  Send: "Отправить",
  "Address copied": "Адрес скопирован",
  "The browser refused to copy the address": "Браузер не дал скопировать адрес",
  "Copy the address": "Скопировать адрес",
  "Since {date}": "С {date}",
  "Nobody can join yet": "Подключиться пока нельзя",
  "Nobody is playing": "Никто не играет",
  "Limit {size}": "Лимит {size}",
  "Memory in use, share of the limit": "Занятая память, доля от лимита",
  "{label}, last three minutes": "{label}, последние три минуты",
  "Host processor": "Процессор машины",
} satisfies Dict;
