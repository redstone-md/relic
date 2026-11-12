import type { Dict } from "@/lib/i18n";

// Settings, news, the new instance form and the loader fields.
export default {
  // Shared buttons
  Cancel: "Отмена",
  Save: "Сохранить",
  Delete: "Удалить",
  Remove: "Удалить",
  Name: "Название",

  // Settings: page
  Settings: "Настройки",
  "How this server presents itself to players and launchers.":
    "Как этот сервер выглядит для игроков и лаунчеров.",
  "Save settings": "Сохранить настройки",
  "Settings saved": "Настройки сохранены",
  "Unsaved changes": "Есть несохранённые изменения",
  "Colours must be six-digit hex, like #fb7013.":
    "Цвета задаются шестизначным hex-кодом, например #fb7013.",

  // Settings: server
  Server: "Сервер",
  "Shown in the launcher's title.": "Показывается в заголовке лаунчера.",
  "Public address": "Публичный адрес",
  "The domain players type into the launcher and connect to in game, without a scheme or port.":
    "Домен, который игроки вводят в лаунчере и по которому подключаются в игре, без схемы и порта.",

  // Settings: registration
  Registration: "Регистрация",
  "Who can create an account": "Кто может создать аккаунт",
  "Registration policy": "Правила регистрации",
  "Invite only": "Только по приглашениям",
  "A new player needs a one-time invite code from you.":
    "Новому игроку нужен одноразовый код приглашения от вас.",
  Approval: "С одобрением",
  "Anyone can sign up, and plays once you approve the account.":
    "Зарегистрироваться может любой, а играть — после того, как вы одобрите аккаунт.",
  Closed: "Закрыта",
  "Nobody new can register.": "Новые игроки зарегистрироваться не могут.",

  // Settings: scheduled backups
  "Scheduled backups": "Резервные копии по расписанию",
  "Every, hours": "Интервал, часы",
  "0 turns the schedule off.": "0 отключает расписание.",
  "Keep the newest": "Хранить копий",
  "Older copies of each instance are deleted.": "Более старые копии каждого сервера удаляются.",

  // Settings: launcher theme
  "Launcher theme": "Тема лаунчера",
  Background: "Фон",
  Surface: "Поверхность",
  Border: "Граница",
  Accent: "Акцент",
  Text: "Текст",
  "Muted text": "Приглушённый текст",
  "{label} colour picker": "{label}: выбор цвета",
  "{label} hex value": "{label}: hex-код",
  Logo: "Логотип",
  "PNG or WebP, https.": "PNG или WebP, https.",
  "Background image": "Фоновая картинка",
  "Wide, https.": "Широкая, https.",
  "Font file": "Файл шрифта",
  "TTF or OTF, https.": "TTF или OTF, https.",
  Preview: "Предпросмотр",
  "Your server": "Ваш сервер",
  "Season two opens Friday": "Второй сезон стартует в пятницу",
  "New world, updated modpack. The launcher downloads only what changed.":
    "Новый мир, обновлённая сборка. Лаунчер скачает только то, что изменилось.",
  "12 players online": "12 игроков онлайн",
  PLAY: "ИГРАТЬ",

  // Settings: launcher links
  "Launcher links": "Ссылки в лаунчере",
  "Buttons on the launcher's home screen: a site, Discord, a map.":
    "Кнопки на главном экране лаунчера: сайт, Discord, карта.",
  "Add link": "Добавить ссылку",
  "No links.": "Ссылок нет.",
  Label: "Подпись",
  "Label of link {number}": "Подпись ссылки {number}",
  "Address of link {number}": "Адрес ссылки {number}",
  "Remove link {number}": "Убрать ссылку {number}",

  // Settings: CurseForge key
  "Modrinth works without a key. Searching CurseForge needs your own API key from":
    "Modrinth работает без ключа. Для поиска в CurseForge нужен свой API-ключ, его выдают на",
  "A key is stored. It is never shown again.": "Ключ сохранён. Больше он не показывается.",
  "No key stored.": "Ключ не задан.",
  "Replace the key": "Заменить ключ",
  "API key": "API-ключ",
  "Save key": "Сохранить ключ",
  "CurseForge key saved": "Ключ CurseForge сохранён",
  "CurseForge key removed": "Ключ CurseForge удалён",
  "Remove the CurseForge key?": "Удалить ключ CurseForge?",
  "CurseForge search and installs stop working until a key is added again.":
    "Поиск и установка из CurseForge не будут работать, пока вы снова не добавите ключ.",

  // News
  News: "Новости",
  "Announcements players see when they open the launcher.":
    "Объявления, которые игроки видят при запуске лаунчера.",
  "New post": "Новая запись",
  "Edit post": "Изменить запись",
  "Shown on the home screen of every launcher.": "Показывается на главном экране каждого лаунчера.",
  Title: "Заголовок",
  "Image address": "Адрес картинки",
  "Optional. An https link to a picture for the post.":
    "Необязательно. Ссылка https на картинку для записи.",
  Publish: "Опубликовать",
  "Post updated": "Запись обновлена",
  "Post published": "Запись опубликована",
  "Post deleted": "Запись удалена",
  "Nothing published yet": "Пока ничего не опубликовано",
  "Tell players about an update, an event or planned downtime.":
    "Расскажите игрокам об обновлении, событии или плановых работах.",
  "Edit {title}": "Изменить «{title}»",
  "Delete {title}": "Удалить «{title}»",
  "Delete this post?": "Удалить эту запись?",
  "“{title}” disappears from every launcher.": "«{title}» исчезнет из всех лаунчеров.",

  // New instance
  "New instance": "Новый сервер",
  "The server files install on first start. Mods and settings can be added before that.":
    "Файлы сервера установятся при первом запуске. Моды и настройки можно добавить до этого.",
  Identity: "Основное",
  "Shown to players in the launcher.": "Игроки видят его в лаунчере.",
  Id: "Идентификатор",
  "Names the folder on disk and the address in the API. Cannot be changed later.":
    "Задаёт имя папки на диске и адрес в API. Позже изменить нельзя.",
  "Use 1-32 lowercase letters, digits and dashes, without a dash at either end.":
    "От 1 до 32 строчных латинских букв, цифр и дефисов, без дефиса в начале и в конце.",
  "Another instance already uses this id.": "Этот идентификатор уже занят другим сервером.",
  Description: "Описание",
  Platform: "Платформа",
  Resources: "Ресурсы",
  Port: "Порт",
  "Leave empty to take the next free port from 25565.":
    "Оставьте пустым, чтобы занять первый свободный порт начиная с 25565.",
  Automatic: "Автоматически",
  "Memory, MB": "Память, МБ",
  "The Java heap limit. Modded servers usually want 4096 or more.":
    "Предел кучи Java. Серверам с модами обычно нужно 4096 и больше.",
  "Create instance": "Создать сервер",

  // Loader fields
  "Minecraft version": "Версия Minecraft",
  "Could not load the version list.": "Не удалось загрузить список версий.",
  "1.12.2 and newer.": "1.12.2 и новее.",
  Loading: "Загрузка",
  "Choose a version": "Выберите версию",
  Loader: "Загрузчик",
  "{loader} build": "Версия {loader}",
  "No {loader} builds for this Minecraft version.": "Для этой версии Minecraft нет версий {loader}.",
  "Loader build": "Версия загрузчика",
  "Newest stable": "Последняя стабильная",
  beta: "бета",
} satisfies Dict;
