# Telegram Bot (Antispam & Moderation)

Цей Telegram-бот допомагає адміністраторам груп модерувати повідомлення: блокує користувачів за заборонені слова, переслані повідомлення з інших чатів і підтримує голосування `/voteban`.

## Функціонал

- Автоматичне видалення повідомлень із забороненими словами.
- Автоблокування користувачів за порушення.
- Автоблокування за переслані повідомлення з інших чатів.
- Ігнорування адміністраторів і власників групи під час автомодерації.
- Команди для адміністраторів боту:
  - `/addword <слова>` - додати заборонені слова.
  - `/listwords` - показати список заборонених слів.
  - `/removeword <слово>` - видалити слово зі списку.
- Команда `/voteban` у відповідь на повідомлення користувача.

## Вимоги

- Docker
- Docker Compose
- Telegram Bot API Token

Бот написаний на Rust і використовує SQLite для зберігання списку заборонених слів.
Docker image збирається в GitHub Actions і публікується в GitHub Container Registry.
Файл бази зберігається в Docker volume, тому переживає оновлення образу та перезапуск контейнера.

## Налаштування

Створіть `.env` у корені репозиторію:

```env
BOT_TOKEN="token from BotFather"
ADMIN_IDS="id_number,id_number,id_number"
SQLITE_PATH="/data/mechabaraholka.sqlite"
VOTEBAN_NEED_COUNT=2
```

`ADMIN_IDS` - Telegram ID адміністраторів боту через кому. Вони можуть керувати списком заборонених слів і отримують службові повідомлення від боту.

## Запуск

Звичайний запуск після налаштування SQLite:

```sh
docker compose pull tgbot
docker compose up -d
```

Після оновлення репозиторію достатньо виконати ті самі команди, щоб завантажити готовий образ `ghcr.io/vivatfreedom/mechabaraholka-bot:latest` і перезапустити бота. На маленькому сервері не використовуйте `docker compose up -d --build`: ця команда збирає Rust-образ локально і може зайняти години.

Якщо GitHub Container Registry package приватний, один раз авторизуйтесь на сервері:

```sh
echo TOKEN | docker login ghcr.io -u vivatfreedom --password-stdin
```

`TOKEN` має мати доступ на читання package.

## Оновлення після міграції з PostgreSQL

Міграцію даних у SQLite вже виконано. PostgreSQL більше не потрібен для роботи бота, а `.env` не має містити `POSTGRES_MIGRATION_URL` або старий PostgreSQL `DATABASE_URL`.

Після оновлення цього compose-файлу запустіть:

```sh
docker compose pull tgbot
docker compose up -d --remove-orphans
```

`--remove-orphans` прибере старий контейнер PostgreSQL з compose-проєкту, але не видалить Docker volumes. Старий PostgreSQL volume можна залишити як резервну копію або видалити вручну пізніше, коли переконаєтесь, що SQLite база містить потрібні слова.

## SQLite persistence

`docker-compose.yml` монтує named volume `bot_data` у `/data`, а бот за замовчуванням використовує файл `/data/mechabaraholka.sqlite`.

- `docker compose pull tgbot` не видаляє SQLite дані.
- `docker compose up -d` не видаляє SQLite дані.
- `docker compose down` не видаляє SQLite дані.
- `docker compose down -v` видаляє named volumes, включно з SQLite базою.

SQLite таблиця:

```sql
CREATE TABLE "Word" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "word" TEXT NOT NULL
);
```
