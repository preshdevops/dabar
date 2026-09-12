export const SAMPLE_TRANSCRIPT_SEGMENTS = [
  { start: 0.0, end: 5.2, text: "Grace and peace to you in the name of our Lord Jesus Christ." },
  { start: 5.2, end: 12.8, text: "Today we are looking at the book of Romans chapter 8 verse 28." },
  { start: 12.8, end: 24.5, text: "For we know that all things work together for good to them that love God." },
  { start: 24.5, end: 38.0, text: "No matter what mountain stands before you this morning, God's promise remains unshakable." },
  { start: 38.0, end: 52.4, text: "When you walk through the fire, you will not be burned; the flames will not set you ablaze." },
  { start: 52.4, end: 68.1, text: "Stand firm in the faith, knowing that the battle belongs to the Lord." },
  { start: 68.1, end: 85.0, text: "Let us pray together and surrender every burden into His capable hands." },
];

export const MOCK_HIGHLIGHTS = [
  {
    id: "3fa85f64-5717-4562-b3fc-2c963f66afa6",
    title: "All Things Work Together For Good",
    start_time: 12.0,
    end_time: 38.0,
    score: 0.95,
    reason: "A deeply resonant theological declaration of God's sovereignty over adversity.",
    suggested_hook_text: "No matter what mountain stands before you, God's promise remains unshakable.",
  },
  {
    id: "7ca85f64-5717-4562-b3fc-2c963f66afa7",
    title: "Walking Through The Fire",
    start_time: 38.0,
    end_time: 68.0,
    score: 0.92,
    reason: "High spiritual encouragement addressing perseverance and faith.",
    suggested_hook_text: "When you walk through the fire, you will not be burned.",
  }
];

export const MOCK_CHAPTERS = [
  {
    id: "4da85f64-5717-4562-b3fc-2c963f66afa8",
    title: "Introduction & Romans 8:28",
    summary: "Opening greeting and scripture foundation.",
    start_time: 0.0,
    end_time: 25.0,
  },
  {
    id: "8ea85f64-5717-4562-b3fc-2c963f66afa9",
    title: "The Fire and the Promise",
    summary: "Exhortation on overcoming trials.",
    start_time: 25.0,
    end_time: 85.0,
  }
];

export const MOCK_LLM_RESPONSES = {
  standardJson: JSON.stringify({
    chapters: [
      { title: "Romans 8 Exposition", summary: "Opening exposition", start_timestamp: 0.0, end_timestamp: 40.0 },
      { title: "Walking in Faith", summary: "Closing prayer", start_timestamp: 40.0, end_timestamp: 85.0 },
    ],
    clips: [
      {
        title: "All Things Work Together",
        start_timestamp: 12.0,
        end_timestamp: 38.0,
        reason: "Climactic pastoral moment.",
        suggested_hook_text: "God works all things together for good.",
        score: 0.96,
      }
    ]
  }),

  fencedJson: "```json\n" + JSON.stringify({
    chapters: [
      { title: "The Sovereign God", summary: "Exposition of grace", start_timestamp: 0.0, end_timestamp: 50.0 }
    ],
    clips: [
      {
        title: "Standing Unshaken",
        start_timestamp: 24.0,
        end_timestamp: 52.0,
        reason: "Key message takeaway",
        suggested_hook_text: "Stand firm in the faith.",
        score: 0.94,
      }
    ]
  }) + "\n```",

  fencedJsonNoLang: "```\n" + JSON.stringify({
    chapters: [
      { title: "Sovereignty of Grace", summary: "Pastoral encouragement", start_timestamp: 0.0, end_timestamp: 60.0 }
    ],
    clips: [
      {
        title: "Through The Fire",
        start_timestamp: 38.0,
        end_timestamp: 68.0,
        reason: "Overcoming hardship",
        suggested_hook_text: "Flames will not set you ablaze.",
        score: 0.93
      }
    ]
  }) + "\n```",

  malformedJson: "Here is your requested pastoral breakdown: { chapters: [ incomplete...",
  emptyResponse: "",
};

export const ADVERSARIAL_SUBTITLES = [
  "He said: 'Do not be afraid!' — It's 100% possible!",
  "God's grace: \"unmerited & boundless\" (Ephesians 2:8-9)",
  "Notice: 50% discount on worldly worries; 100% peace in Christ \\ [Amen]!",
  "Pastoral Quote: “Grace isn’t cheap; it cost everything!” 🔥🙌",
  "Special chars & math: 10 > 5 && 5 < 10, a%b == 0, 'quoted' \"double\"",
];
