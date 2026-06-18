// Centralized dayjs setup. Importing this module (anywhere) guarantees the
// relativeTime plugin is registered and the non-default locales are loaded
// exactly once. English is dayjs's built-in default; Ukrainian is pulled in via
// `dayjs/locale/uk`. The active locale is switched from i18n's setLocale so
// relative times ("5 minutes ago" / "5 хвилин тому") follow the UI language.

import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import "dayjs/locale/uk";

dayjs.extend(relativeTime);

export default dayjs;
