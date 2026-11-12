# Лаунчер Relic, русский.

language-name = Русский
language = Язык
language-auto = Авто
language-follows-system = Авто, { $name }


## Подключение и вход

connect-hint = Введите адрес сервера, на котором играете.
server-address = Адрес сервера
connect = Подключиться
register-approval-hint = Создайте аккаунт. Играть можно после того, как его одобрит админ.
register-invite-hint = Создайте аккаунт по коду приглашения.
reset-hint = Попросите у админа код сброса и задайте новый пароль.
sign-in-hint = Войдите, чтобы играть.
username = Ник
password = Пароль
new-password = Новый пароль
current-password = Текущий пароль
reset-code = Код сброса
code-from-admin = Код от админа
invite-code = Код приглашения
sign-in = Войти
create-account = Создать аккаунт
change-password = Сменить пароль
reset-password = Сбросить пароль
other-server = Другой сервер
game-folder = Папка игры
game-folder-hint = Здесь лежат сборки, файлы игры и Java. Файлы в прежней папке остаются на месте.
open-folder = Открыть
change-folder = Сменить
default-folder = По умолчанию
put-game-here = Хранить игру здесь
enter-credentials = Введите ник и пароль.
not-approved = Админ ещё не одобрил этот аккаунт.
pick-credentials = Придумайте ник и пароль.
needs-invite = Этому серверу нужен код приглашения.
account-created = Аккаунт создан. Войти можно после того, как его одобрит админ.
enter-reset = Введите ник, код и новый пароль.
password-changed-sign-in = Пароль изменён. Войдите с новым.
session-ended = Сессия закончилась. Войдите снова.

## Главная

online = Онлайн
starting = Запуск
installing = Установка
stopping = Остановка
offline = Выключен
down = Сбой
game-with-loader = Minecraft { $version } на { $loader }
online-playing = Онлайн, играют { $online } из { $max }
nothing-to-play = На этом сервере пока не во что играть.
servers = Серверы
server = Сервер
playing-now = сейчас играют
mods =
    { $count ->
        [one] { $count } мод
        [few] { $count } мода
       *[many] { $count } модов
    }
news = Новости
news-dated = Новости, { $date }
no-news = Новостей пока нет
change-skin = Сменить скин
play = ИГРАТЬ
playing = В ИГРЕ
try-again = ЕЩЁ РАЗ
restart-to-update = Перезапустить и обновить до Relic { $version }

## Скин

skin = Скин
arms = Руки
arms-classic = Обычные
arms-slim = Тонкие
arms-hint = Применится к следующему загруженному скину
upload-skin = Загрузить скин
remove = Убрать
file = Файл
skin-file-hint = PNG 64×64, тот же файл, что использует сама игра.
turn-hint = Ведите курсором по фигуре, чтобы повернуть её.
skin-updated = Скин обновлён.
skin-removed = Скин убран.
cannot-read-file = не удалось прочитать файл: { $reason }
skin-too-big = этот файл слишком велик для скина

## Настройки

settings = Настройки
memory = Память
memory-hint = Сколько памяти можно занять игре. Оставьте немного всему остальному.
gigabytes = { $amount } ГБ
megabytes = { $amount } МБ
block-is-gigabyte = Один блок - один гигабайт
optional-mods = Необязательные моды
mods-on = из { $all }
mods-rule = Включайте что хотите. Если моду нужны другие моды, они ставятся вместе с ним и остаются включёнными, пока включён он.
also-installs = Вместе с ним ставится: { $mods }
needed-by = Включён, потому что нужен для: { $mods }
needed-by-pack = Включён, потому что нужен сборке
no-mods = Нет
password-hint = Выберите такой, какого у вас нет больше нигде.
password-changed = Пароль изменён.
enter-passwords = Введите текущий пароль и новый.
wrong-current-password = Текущий пароль неверен.
account = Аккаунт
on-server = На { $address }
sign-out = Выйти

## Связь с сервером

address-empty = введите адрес сервера
address-invalid = это не похоже на адрес сервера
address-insecure = к этому серверу нужно подключаться по https
network-failed = не удалось связаться с сервером: { $reason }
protocol-failed = сервер прислал что-то неожиданное: { $reason }
launcher-outdated = этому серверу нужен лаунчер новее
timed-out = время ожидания вышло
nothing-answered = по этому адресу никто не ответил

## Подготовка игры

checking-updates = Проверка обновлений…
checking-mods = Проверка модов…
downloading-mods = Загрузка модов ({ $size })…
signing-in-to-game = Вход в игру…
starting-game = Запуск игры…
installing-loader = Установка загрузчика модов…
checking-game-files = Проверка файлов игры…
downloading-java = Загрузка Java…
start-failed = не удалось запустить игру: { $reason }
instance-name-unusable = лаунчер не может сохранить сборку с таким именем
stopped-with-code = Игра остановилась с кодом { $code }. Её лог: { $log }
stopped-unexpectedly = Игра неожиданно остановилась. Её лог: { $log }
path-unusable = сервер указал файл с негодным путём: { $path }
download-failed = не удалось скачать { $path }: { $reason }
server-answered = сервер ответил { $status }
corrupt = файл { $path } пришёл повреждённым
files-failed = не удалось обновить файлы игры: { $reason }

## Даты

date = { $day } { $month } { $year }
month-1 = янв
month-2 = фев
month-3 = мар
month-4 = апр
month-5 = мая
month-6 = июн
month-7 = июл
month-8 = авг
month-9 = сен
month-10 = окт
month-11 = ноя
month-12 = дек
