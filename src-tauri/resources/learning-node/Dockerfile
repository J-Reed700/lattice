FROM node@sha256:43ac6c60b8f89723f746e8a92ce91abd5017e627ce1ddfe4238355d3a30b772c

WORKDIR /opt/lattice
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund

ENV NODE_PATH=/opt/lattice/node_modules
LABEL com.lattice.learning.runtime="react-typescript"
