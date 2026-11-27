import raw from "../fixtures/review.json";
import type { Review } from "../types";
import { normalizeReview } from "./review";

export const fixtureReview: Review = normalizeReview(raw as unknown as Review);
