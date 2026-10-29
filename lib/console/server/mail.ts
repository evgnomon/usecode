// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Sending mail: the login codes, over SMTP.

import { createTransport } from "nodemailer";
import type { Config, Smtp } from "./config.js";

export interface Mailer {
  send(to: string, subject: string, text: string): Promise<void>;
}

/** Mail through `smtp`, with TLS from the first byte on 465, STARTTLS on other ports. */
export function smtp(s: Smtp): Mailer {
  const transport = createTransport({
    host: s.host,
    port: s.port,
    secure: s.port === 465,
    requireTLS: true,
    auth: { user: s.user, pass: s.password },
    connectionTimeout: 10_000,
    greetingTimeout: 10_000,
    socketTimeout: 20_000,
  });
  return {
    async send(to, subject, text) {
      await transport.sendMail({ from: s.from, to, subject, text });
    },
  };
}

/** For trying it out on localhost: the mail goes to the log. */
export const toLog: Mailer = {
  async send(to, subject, text) {
    console.log(`mail to ${to}: ${subject}\n${text}`);
  },
};

/** Without SMTP on a public console: nothing to send with, so codes can't go out. */
export const off: Mailer = {
  async send() {
    throw new Error("mail is off: set SMTP_PASSWORD");
  },
};

export function fromConfig(cfg: Config): Mailer {
  if (cfg.smtp) return smtp(cfg.smtp);
  return cfg.secureCookies ? off : toLog;
}
