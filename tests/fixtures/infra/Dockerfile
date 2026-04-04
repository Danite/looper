FROM node:20-alpine AS builder

RUN apk add --no-cache \
    ffmpeg \
    imagemagick \
    git \
    curl

COPY package.json yarn.lock ./
COPY src/ ./src/

RUN npm install

EXPOSE 3000 8080

FROM node:20-alpine AS production

COPY --from=builder /app /app

EXPOSE 3000
