import { Bot, Context, GrammyError, HttpError } from "grammy";
import "dotenv/config";
import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();
const bot = new Bot(process.env.BOT_TOKEN as string);

const ADMIN_IDS = (process.env.ADMIN_IDS || "")
  .split(",")
  .map((id) => id.trim());

async function log(message: string) {
  console.log(message);
  for (const adminId of ADMIN_IDS) {
    try {
      await bot.api.sendMessage(adminId, message);
    } catch (err) {
      console.error(
        `Помилка при надсиланні повідомлення адміну ${adminId}:`,
        err
      );
    }
  }
}

async function isGroupAdmin(ctx: Context, userId: number): Promise<boolean> {
  try {
    const member = await ctx.api.getChatMember(ctx.chat!.id, userId);
    return ["administrator", "creator"].includes(member.status);
  } catch (error) {
    await log(`Помилка при перевірці прав користувача ${userId}: ${error}`);
    return false;
  }
}

async function containsBanWords(text: string): Promise<boolean> {
  const words = await prisma.word.findMany();
  const banWords = words.map((word) => word.word.toLowerCase());
  return banWords.some((word) => text.toLowerCase().includes(word));
}

bot.command("addword", async (ctx) => {
  if (!ADMIN_IDS.includes(ctx.from!.id.toString())) {
    await ctx.reply("Тільки адміністратори можуть додавати слова.");
    return;
  }
  const words = ctx.match?.trim().split(/[,; ]+/); // Розділяємо на слова через кому, крапку з комою або пробіл
  if (!ctx.match || !words || words.length === 0) {
    await ctx.reply(
      "Будь ласка, вкажіть хоча б одне слово після команди /addword."
    );
    return;
  }

  // Перевіряємо, чи існують слова в базі, і додаємо їх
  let addedCount = 0;
  for (const word of words) {
    const trimmedWord = word.trim().toLowerCase();
    if (!trimmedWord) continue; // Пропускаємо порожні елементи

    const exists = await prisma.word.findFirst({
      where: { word: trimmedWord },
    });

    if (!exists) {
      await prisma.word.create({ data: { word: trimmedWord } });
      addedCount++;
    }
  }

  if (addedCount > 0) {
    await ctx.reply(`Додано ${addedCount} нових слів.`);
    await log(
      `@${ctx.from!.username}: Додав ${addedCount} ${
        addedCount === 1 ? "нове слово" : "нових слів"
      }: ${words}`
    );
  } else {
    await ctx.reply(
      "Жодне нове слово не було додано (можливо, всі вже є в списку)."
    );
  }
});

bot.command("listwords", async (ctx) => {
  if (!ADMIN_IDS.includes(ctx.from!.id.toString())) {
    await ctx.reply("Тільки адміністратори можуть дивитися слова.");
    return;
  }
  const words = await prisma.word.findMany();
  if (words.length === 0) {
    await ctx.reply("Список слів порожній.");
    return;
  }

  const wordList = words.map((w) => w.word).join(", ");
  await ctx.reply(`Заборонені слова: ${wordList}`);
});

bot.command("removeword", async (ctx) => {
  if (!ADMIN_IDS.includes(ctx.from!.id.toString())) {
    await ctx.reply("Тільки адміністратори можуть видаляти слова.");
    return;
  }
  const word = ctx.match?.trim();
  if (!word) {
    await ctx.reply("Будь ласка, вкажіть слово після команди /removeword");
    return;
  }

  const deleted = await prisma.word.deleteMany({ where: { word } });
  if (deleted.count > 0) {
    await ctx.reply(`Слово "${word}" видалено зі списку.`);
    await log(`@${ctx.from!.username}: Видалив слово ${word} зі списку.`);
  } else {
    await ctx.reply(`Слово "${word}" не знайдено.`);
  }
});

bot.on("message", async (ctx) => {
  const message = ctx.message;
  if (!message || !message.from) return;

  const userId = message.from.id;
  const username = message.from.username || "Без імені";
  const text = message.text || "";

  const isAdmin = await isGroupAdmin(ctx, userId);
  if (isAdmin) {
    await log(`@${username} (${userId}) - адміністратор, дії не виконуються.`);
    return;
  }

  if (
    "forward_from_chat" in message &&
    message.forward_from_chat &&
    (message.forward_from_chat as { id: number })?.id !== ctx.chat!.id
  ) {
    await log(
      `Переслане повідомлення від @${username} (${userId}). Блокування.`
    );
    await banUser(ctx, userId, message.message_id);
  } else if (await containsBanWords(text)) {
    await log(
      `Заборонене слово в повідомленні від @${username} (${userId}). Блокування.`
    );
    await banUser(ctx, userId, message.message_id);
  }
});

async function banUser(ctx: Context, userId: number, messageId: number) {
  try {
    await ctx.api.deleteMessage(ctx.chat!.id, messageId);
    await ctx.api.banChatMember(ctx.chat!.id, userId);

    await log(`Користувач ${userId} заблокований.`);
  } catch (error) {
    await log(
      `Не вдалось заблокувати користувача @${ctx.from?.username} ${userId}.`
    );
    const errorMessage =
      error instanceof GrammyError
        ? `Помилка API: ${error.message}`
        : error instanceof HttpError
        ? `Помилка мережі: ${error.message}`
        : `Невідома помилка: ${error}`;
    await log(errorMessage);
  }
}

async function startBot() {
  bot.start().catch(async (err) => log(`Проблеми: ${err}`));
  await log("Бот успішно запущений!");
}

startBot();

process.once("SIGINT", async () => {
  await prisma.$disconnect();
  bot.stop();
});
process.once("SIGTERM", async () => {
  await prisma.$disconnect();
  bot.stop();
});
