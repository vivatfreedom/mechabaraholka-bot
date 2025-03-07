FROM node:20-alpine
RUN mkdir -p /opt/app
WORKDIR /opt/app
COPY package.json .
COPY tsconfig.json .
COPY .env .
RUN npm install
COPY src/ ./src
COPY prisma/ ./prisma
CMD ["sh", "-c", "npx prisma migrate deploy && npx prisma generate && npm start"]