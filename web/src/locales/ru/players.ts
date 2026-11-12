import type { Dict } from "@/lib/i18n";

// Words of the players page (accounts, invites) and of the sign-in and
// first-run screens.
export default {
  // Statuses of an account.
  Active: "Активен",
  "Awaiting approval": "Ждёт одобрения",
  Banned: "Заблокирован",

  // How long an invite lives.
  "1 day": "1 день",
  "3 days": "3 дня",
  "7 days": "7 дней",
  "30 days": "30 дней",
  "Never expires": "Бессрочно",

  // A secret shown once.
  "Could not copy. Select the code and copy it by hand.":
    "Не удалось скопировать. Выделите код и скопируйте его вручную.",
  Copied: "Скопировано",
  Copy: "Копировать",

  // New account.
  "Account {name} created": "Аккаунт {name} создан",
  "New account": "Новый аккаунт",
  "Created active right away, whatever the registration policy says.":
    "Аккаунт сразу активен, какой бы ни была политика регистрации.",
  Username: "Ник",
  "3-16 characters: letters, digits and underscore. This is the in-game name.":
    "3–16 символов: латинские буквы, цифры и подчёркивание. Это имя в игре.",
  Password: "Пароль",
  "At least 8 characters. Pass it to the player; they can change it in the launcher.":
    "Не меньше 8 символов. Передайте пароль игроку — он сможет сменить его в лаунчере.",
  Role: "Роль",
  Player: "Игрок",
  "Admin: full access to this panel": "Администратор: полный доступ к панели",
  Cancel: "Отмена",
  Create: "Создать",

  // New invite.
  "Invite ready": "Приглашение готово",
  "Send this code to the player. It works once and is not shown again.":
    "Отправьте этот код игроку. Он сработает один раз и больше не будет показан.",
  "Copy the invite code": "Скопировать код приглашения",
  Done: "Готово",
  "New invite": "Новое приглашение",
  "A one-time code that lets one person register an account.":
    "Одноразовый код, по которому один человек может зарегистрировать аккаунт.",
  Note: "Заметка",
  "Optional. Who the invite is for; only admins see it.":
    "Необязательно. Для кого приглашение; заметку видят только администраторы.",
  "Valid for": "Срок действия",
  "Create invite": "Создать приглашение",

  // Password reset.
  "Reset code for {name}": "Код сброса для {name}",
  "The player enters it in the launcher under “Forgot password” and picks a new password. Valid until {date}; not shown again.":
    "Игрок вводит его в лаунчере в разделе «Забыли пароль» и задаёт новый пароль. Код действует до {date} и больше не будет показан.",
  "Copy the reset code": "Скопировать код сброса",

  // Accounts.
  "{name} is now an admin": "{name} теперь администратор",
  "{name} is now a player": "{name} теперь игрок",
  "{name} banned": "Аккаунт {name} заблокирован",
  "{name} can play now": "{name} может играть",
  "Account {name} deleted": "Аккаунт {name} удалён",
  "Find a player": "Найти игрока",
  "No player matches": "Никого не найдено",
  Status: "Статус",
  Now: "Сейчас",
  Registered: "Регистрация",
  Actions: "Действия",
  Admin: "Администратор",
  you: "вы",
  Offline: "Не в сети",
  Approve: "Одобрить",
  "Actions for {name}": "Действия: {name}",
  "Lift the ban": "Разблокировать",
  Ban: "Заблокировать",
  "Make a player": "Сделать игроком",
  "Make an admin": "Сделать администратором",
  "Reset password": "Сбросить пароль",
  "Delete account": "Удалить аккаунт",
  "Ban {name}?": "Заблокировать {name}?",
  "They are signed out everywhere, kicked from running servers and cannot sign in until the ban is lifted.":
    "Игрок выйдет из аккаунта на всех устройствах, будет отключён от запущенных серверов и не сможет войти, пока блокировка не снята.",
  "Delete {name}?": "Удалить {name}?",
  "The account, its skin and its sessions are removed for good. The name becomes free to register again. Worlds keep the player's items.":
    "Аккаунт, его скин и сессии будут удалены навсегда. Ник снова можно будет зарегистрировать. Вещи игрока в мирах сохранятся.",
  Delete: "Удалить",

  // Invites.
  "Used by {name}": "Использовано: {name}",
  "Used by a deleted account": "Использовано удалённым аккаунтом",
  Expired: "Истекло",
  "Open ## invite state": "Действует",
  "Invite deleted": "Приглашение удалено",
  "No invites yet": "Приглашений пока нет",
  "An invite is a one-time code. Under the “invite only” registration policy nobody can sign up without one.":
    "Приглашение — это одноразовый код. При политике регистрации «только по приглашениям» без него зарегистрироваться нельзя.",
  State: "Состояние",
  Created: "Создано",
  Expires: "Истекает",
  "No note": "Без заметки",
  "{date} by {name}": "{date}, {name}",
  Never: "Никогда",
  "Delete the invite": "Удалить приглашение",
  "Delete this invite?": "Удалить это приглашение?",
  "Only the record goes away; the account made with it stays.":
    "Удалится только запись; аккаунт, созданный по приглашению, останется.",
  "The code stops working.": "Код перестанет работать.",

  // The page itself.
  Players: "Игроки",
  "Accounts registered on this server. The username is the in-game name.":
    "Аккаунты, зарегистрированные на этом сервере. Ник — это имя в игре.",
  Accounts: "Аккаунты",
  Invites: "Приглашения",

  // First-run setup and sign-in.
  "First-run setup": "Первый запуск",
  "Create the administrator account. The setup token was printed to the daemon's terminal when it started.":
    "Создайте аккаунт администратора. Токен настройки был выведен в терминал демона при запуске.",
  "Setup token": "Токен настройки",
  "3-16 characters: letters, digits, underscore. Also your in-game name.":
    "3–16 символов: латинские буквы, цифры, подчёркивание. Это же ваше имя в игре.",
  "Repeat password": "Повторите пароль",
  "The passwords differ.": "Пароли не совпадают.",
  "Create administrator": "Создать администратора",
  "Sign in": "Войти",
  "Administrator access to this server.": "Доступ администратора к этому серверу.",
} satisfies Dict;
