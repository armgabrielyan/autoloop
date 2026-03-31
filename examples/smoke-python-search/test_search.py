import unittest

from search import build_demo_catalog, search_catalog


class SearchCatalogTests(unittest.TestCase):
    def setUp(self):
        self.catalog = build_demo_catalog(40)

    def test_python_queries_rank_python_notes_first(self):
        results = search_catalog(self.catalog, "python cache performance", limit=3)
        self.assertEqual(len(results), 3)
        self.assertTrue(all(title.startswith("Python Caching") for title in results))

    def test_search_respects_limit(self):
        results = search_catalog(self.catalog, "search ranking query", limit=2)
        self.assertEqual(len(results), 2)

    def test_search_returns_empty_when_no_documents_match(self):
        results = search_catalog(self.catalog, "quantum ledger banana", limit=5)
        self.assertEqual(results, [])


if __name__ == "__main__":
    unittest.main()
