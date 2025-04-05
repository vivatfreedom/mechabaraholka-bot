import { Bot, Context, GrammyError, HttpError, InlineKeyboard } from "grammy";
import "dotenv/config";
import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();
const bot = new Bot(process.env.BOT_TOKEN as string);

const ADMIN_IDS = (process.env.ADMIN_IDS || "")
  .split(",")
  .map((id) => id.trim());

const VOTEBAN_NEED_COUNT = process.env.VOTEBAN_NEED_COUNT
  ? parseInt(process.env.VOTEBAN_NEED_COUNT)
  : 2;

const activeVotebans = new Map<
  number,
  {
    targetUserId: number;
    targetMessageId: number;
    voters: Map<number, boolean>;
    votebanMessageId: number;
    initiatorId: number;
    targetUsername: string;
  }
>();

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
  const words = ctx.match?.trim().split(/[,; ]+/);
  if (!ctx.match || !words || words.length === 0) {
    await ctx.reply(
      "Будь ласка, вкажіть хоча б одне слово після команди /addword."
    );
    return;
  }

  let addedCount = 0;
  for (const word of words) {
    const trimmedWord = word.trim().toLowerCase();
    if (!trimmedWord) continue;

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

bot.command("voteban", async (ctx) => {
  if (ctx.chat?.type === "private") {
    await ctx.reply("Ця команда працює тільки в групових чатах.");
    return;
  }

  if (!ctx.message?.reply_to_message) {
    await ctx.reply("Будь ласка, відповідьте на повідомлення для /voteban");
    return;
  }

  const targetMessage = ctx.message.reply_to_message;
  const targetUserId = targetMessage.from?.id;
  const chatId = ctx.chat?.id;
  const initiatorId = ctx.from?.id;
  const targetUsername = targetMessage.from?.username || "Користувач";

  if (!targetUserId || !chatId || !initiatorId) {
    await ctx.reply("Не вдалося визначити користувача або чат");
    return;
  }

  if (await isGroupAdmin(ctx, targetUserId)) {
    await log(`@${ctx.from?.username} спробував банити адміністратора.`);
    await ctx.api.deleteMessage(chatId, ctx.message.message_id);
    return;
  }

  try {
    await ctx.api.deleteMessage(chatId, ctx.message.message_id);
  } catch (error) {
    console.error("Не вдалося видалити повідомлення /voteban:", error);
  }

  const voters = new Map<number, boolean>();
  voters.set(initiatorId, true);

  const initiatorUsername = ctx.from?.username || "Користувач";

  const votebanMessage = await ctx.reply(
    `🗳️ Голосування за бан @${targetUsername}\n\n` +
      `✅ За (1/${VOTEBAN_NEED_COUNT}): @${initiatorUsername}\n` +
      `❌ Проти (0/${VOTEBAN_NEED_COUNT}):`,
    {
      reply_to_message_id: targetMessage.message_id,
      reply_markup: new InlineKeyboard()
        .text(`✅ За (1/${VOTEBAN_NEED_COUNT})`, "vote_ban")
        .text(`❌ Проти (0/${VOTEBAN_NEED_COUNT})`, "vote_against"),
    }
  );

  activeVotebans.set(votebanMessage.message_id, {
    targetUserId,
    targetMessageId: targetMessage.message_id,
    voters,
    votebanMessageId: votebanMessage.message_id,
    initiatorId,
    targetUsername,
  });
});

async function updateVotebanMessage(
  ctx: Context,
  votebanInfo: {
    targetUserId: number;
    voters: Map<number, boolean>;
    votebanMessageId: number;
    targetUsername: string;
  }
) {
  const chatId = ctx.chat?.id;
  if (!chatId) return;

  const proVotes = Array.from(votebanInfo.voters.entries())
    .filter(([_, vote]) => vote)
    .map(([userId]) => userId);

  const againstVotes = Array.from(votebanInfo.voters.entries())
    .filter(([_, vote]) => !vote)
    .map(([userId]) => userId);

  const proUsernames = await Promise.all(
    proVotes.map(async (userId) => {
      try {
        const user = await ctx.api.getChatMember(chatId, userId);
        return `@${user.user.username || `Користувач`}`;
      } catch {
        return `Користувач`;
      }
    })
  );

  const againstUsernames = await Promise.all(
    againstVotes.map(async (userId) => {
      try {
        const user = await ctx.api.getChatMember(chatId, userId);
        return `@${user.user.username || `Користувач`}`;
      } catch {
        return `Користувач`;
      }
    })
  );

  const newText =
    `🗳️ Голосування за бан @${votebanInfo.targetUsername}\n\n` +
    `✅ За (${proVotes.length}/${VOTEBAN_NEED_COUNT}): ${
      proUsernames.join(", ") || "немає"
    }\n` +
    `❌ Проти (${againstVotes.length}/${VOTEBAN_NEED_COUNT}): ${
      againstUsernames.join(", ") || "немає"
    }`;

  const newMarkup = new InlineKeyboard()
    .text(`✅ За (${proVotes.length}/${VOTEBAN_NEED_COUNT})`, "vote_ban")
    .text(
      `❌ Проти (${againstVotes.length}/${VOTEBAN_NEED_COUNT})`,
      "vote_against"
    );

  try {
    // Спроба оновити повідомлення без попередньої перевірки
    await ctx.api.editMessageText(
      chatId,
      votebanInfo.votebanMessageId,
      newText,
      {
        reply_markup: newMarkup,
      }
    );
  } catch (error) {
    // Ігноруємо помилку "message is not modified"
    if (
      !(
        error instanceof GrammyError &&
        error.description.includes("message is not modified")
      )
    ) {
      console.error("Помилка при оновленні повідомлення:", error);
    }
  }
}

bot.callbackQuery(["vote_ban", "vote_against"], async (ctx) => {
  const votebanMessageId = ctx.callbackQuery.message?.message_id;
  if (!votebanMessageId) return;

  const votebanInfo = activeVotebans.get(votebanMessageId);
  if (!votebanInfo) return;

  const userId = ctx.callbackQuery.from.id;
  const chatId = ctx.chat?.id;
  if (!chatId) return;

  if (userId === votebanInfo.targetUserId) {
    await ctx.answerCallbackQuery("Ви не можете голосувати за себе!");
    return;
  }

  // Перевіряємо, чи користувач вже голосував і чи змінив свій голос
  const previousVote = votebanInfo.voters.get(userId);
  const isBanVote = ctx.callbackQuery.data === "vote_ban";

  // Якщо голос той самий, просто виходимо
  if (previousVote === isBanVote) {
    await ctx.answerCallbackQuery("Ви вже голосували");
    return;
  }

  votebanInfo.voters.set(userId, isBanVote);
  await ctx.answerCallbackQuery();

  await updateVotebanMessage(ctx, votebanInfo);

  const proVotes = Array.from(votebanInfo.voters.values()).filter(
    (vote) => vote
  ).length;

  const againstVotes = Array.from(votebanInfo.voters.values()).filter(
    (vote) => !vote
  ).length;

  if (proVotes >= VOTEBAN_NEED_COUNT) {
    try {
      await ctx.api.banChatMember(chatId, votebanInfo.targetUserId);
      await ctx.api.deleteMessage(chatId, votebanInfo.targetMessageId);
      await ctx.api.deleteMessage(chatId, votebanInfo.votebanMessageId);
      await log(
        `Користувач @${votebanInfo.targetUsername} заблокований через голосування.`
      );
    } catch (error) {
      await log(
        `Не вдалося заблокувати користувача ${votebanInfo.targetUsername}: ${error}`
      );
    } finally {
      activeVotebans.delete(votebanMessageId);
    }
  }

  if (againstVotes >= VOTEBAN_NEED_COUNT) {
    try {
      await ctx.api.deleteMessage(chatId, votebanInfo.votebanMessageId);
      await log(
        `Користувач @${votebanInfo.targetUsername} не був заблокований через голосування.`
      );
    } catch (error) {
      await log(`Не вдалося видалити повідомлення з голосуванням: ${error}`);
    }
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
