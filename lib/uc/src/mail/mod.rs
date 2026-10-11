// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc mail`: looking after the mail server the `mail` role of
//! `uc kube configure` runs. [`configure`] keeps its DNS, reverse DNS and
//! Stalwart settings in shape, from the [`records`] mail needs, with
//! [`stalwart`]'s management API, and [`settings`] is what mail apps need
//! to use it.

pub mod configure;
pub mod records;
pub mod settings;
pub mod stalwart;
