// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc mail`: looking after the mail server the `mail` role of
//! `uc kube sync` runs. [`configure`] keeps its DNS, reverse DNS and
//! Stalwart settings in shape, from the [`records`] mail needs, with
//! [`stalwart`]'s management API, and [`settings`] is what mail apps need
//! to use it. [`manage`] looks after its domains and accounts, and
//! [`inbox`] pulls the mail of any IMAP account to this machine to read,
//! and [`compose`] sends mail and replies over SMTP.

pub mod compose;
pub mod configure;
pub mod inbox;
pub mod manage;
pub mod records;
pub mod settings;
pub mod stalwart;
