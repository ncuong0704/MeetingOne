/** Full-page splash unmounts dialogs such as Người nói. Use only before the first payload. */
export function shouldSplashMeetingDetails(hasMeetingDetails: boolean): boolean {
  return !hasMeetingDetails;
}
