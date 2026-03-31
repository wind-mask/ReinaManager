/**
 * @file TAG翻译工具
 * @description 提供VNDB TAG的中文翻译功能
 * @module src/utils/tagTranslation
 * @author Pysio<qq593277393@outlook.com>
 * @copyright AGPL-3.0
 */

import vndbTagTranslationsRaw from "@/locales/_VndbTag_zh_CN.json";

// 过滤掉元数据字段，只保留翻译内容
const vndbTagTranslations = Object.fromEntries(
  Object.entries(vndbTagTranslationsRaw).filter(([key]) => !key.startsWith("_")),
) as Record<string, string>;

export function getTagDisplayName(tag: string, enableTranslation: boolean): string {
  if (!enableTranslation) {
    return tag;
  }

  return vndbTagTranslations[tag] ?? tag;
}
