# Relic launcher, English.
#
# This file has every message and stands in for whatever another language
# leaves out. To add a language, copy it to <code>.ftl (ru.ftl, pt-BR.ftl),
# translate the right-hand sides and keep the { $names } as they are.
# The format is Fluent: https://projectfluent.org/fluent/guide/

# What the language calls itself, shown in the list of languages.
language-name = English
language = Language
language-auto = Auto
language-follows-system = Auto, { $name }


## Connecting and signing in

connect-hint = Enter the address of the server you play on.
server-address = Server address
connect = Connect
register-approval-hint = Create an account. An admin approves it before you can play.
register-invite-hint = Create an account with your invite code.
reset-hint = Ask an admin for a reset code, then choose a new password.
sign-in-hint = Sign in to play.
username = Username
password = Password
new-password = New password
current-password = Current password
reset-code = Reset code
code-from-admin = Code from an admin
invite-code = Invite code
sign-in = Sign in
create-account = Create account
change-password = Change password
reset-password = Reset password
other-server = Other server
game-folder = Game folder
game-folder-hint = Modpacks, game files and Java are kept here. Files in the old folder stay where they are.
open-folder = Open
change-folder = Change
default-folder = Default
put-game-here = Keep the game here
enter-credentials = Enter your username and password.
not-approved = An admin has not approved this account yet.
pick-credentials = Pick a username and a password.
needs-invite = This server needs an invite code.
account-created = Account created. You can sign in once an admin approves it.
enter-reset = Enter your username, the code and a new password.
password-changed-sign-in = Password changed. Sign in with the new one.
session-ended = Your session ended. Sign in again.

## Home

online = Online
starting = Starting
installing = Installing
stopping = Stopping
offline = Offline
down = Down
game-with-loader = Minecraft { $version } with { $loader }
online-playing = Online, { $online } of { $max } playing
nothing-to-play = This server has nothing to play yet.
servers = Servers
server = Server
playing-now = playing now
mods =
    { $count ->
        [one] { $count } mod
       *[other] { $count } mods
    }
news = News
news-dated = News, { $date }
no-news = No news yet
change-skin = Change skin
play = PLAY
playing = PLAYING
try-again = TRY AGAIN
restart-to-update = Restart to use Relic { $version }

## Skin

skin = Skin
arms = Arms
arms-classic = Classic
arms-slim = Slim
arms-hint = Applies to the next skin you upload
upload-skin = Upload a skin
remove = Remove
file = File
skin-file-hint = A 64×64 PNG, the same file the game itself uses.
turn-hint = Move the pointer across the figure to turn it.
skin-updated = Skin updated.
skin-removed = Skin removed.
cannot-read-file = cannot read the file: { $reason }
skin-too-big = that file is too big to be a skin

## Settings

settings = Settings
memory = Memory
memory-hint = How much memory the game may use. Leave some for everything else.
gigabytes = { $amount } GB
megabytes = { $amount } MB
block-is-gigabyte = One block is a gigabyte
optional-mods = Optional mods
mods-on = of { $all }
mods-rule = Switch on what you want. A mod that needs other mods brings them along, and they stay on while it does.
also-installs = Also installs { $mods }
needed-by = On because { $mods } needs it
needed-by-pack = On because the pack needs it
no-mods = None
password-hint = Pick one you do not use anywhere else.
password-changed = Password changed.
enter-passwords = Enter your current password and a new one.
wrong-current-password = The current password is wrong.
account = Account
on-server = On { $address }
sign-out = Sign out

## Reaching the server

address-empty = enter the server address
address-invalid = that does not look like a server address
address-insecure = this server has to be reached over https
network-failed = could not reach the server: { $reason }
protocol-failed = the server sent something unexpected: { $reason }
launcher-outdated = this server needs a newer launcher
timed-out = the connection timed out
nothing-answered = nothing answered at that address

## Getting the game ready

checking-updates = Checking for updates…
checking-mods = Checking mods…
downloading-mods = Downloading mods ({ $size })…
signing-in-to-game = Signing in to the game…
starting-game = Starting the game…
installing-loader = Installing mod loader…
checking-game-files = Checking game files…
downloading-java = Downloading Java…
start-failed = could not start the game: { $reason }
instance-name-unusable = this server uses an instance name the launcher cannot store
stopped-with-code = The game stopped with code { $code }. Its log is at { $log }
stopped-unexpectedly = The game stopped unexpectedly. Its log is at { $log }
path-unusable = the server lists a file with an unusable path: { $path }
download-failed = could not download { $path }: { $reason }
server-answered = the server answered { $status }
corrupt = { $path } arrived damaged
files-failed = could not update game files: { $reason }

## Dates

# 12 Nov 2026
date = { $day } { $month } { $year }
month-1 = Jan
month-2 = Feb
month-3 = Mar
month-4 = Apr
month-5 = May
month-6 = Jun
month-7 = Jul
month-8 = Aug
month-9 = Sep
month-10 = Oct
month-11 = Nov
month-12 = Dec
