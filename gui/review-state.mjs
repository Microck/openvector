/** Window-owned review preference. Only checked state crosses the storage and
 * HTTP boundaries; the document and wizard page remain session state. */
export function readReviewPreferences(input) {
  if(!input || typeof input!=='object' || Array.isArray(input) || Object.keys(input).length!==1 ||
      !Object.hasOwn(input,'skipReview') || typeof input.skipReview!=='boolean')
    throw new Error('Review preferences must contain one Boolean skipReview choice.');
  return {skipReview:input.skipReview};
}

export function completedResultPage(settings,skipReview) {
  return settings.advanced || skipReview ? 'export' : 'review';
}

/** Native result-page Back differs from ordinary wizard history. The caller
 * supplies Review or Export, and owns editor entry when background is returned. */
export function resultBackPage(page,settings,skipReview) {
  const earlier=settings.paletteMode===1 ? 'palette' : 'colors';
  if(page==='export') {
    if(settings.advanced)return 'background';
    return skipReview ? earlier : 'review';
  }
  return skipReview ? 'export' : earlier;
}
