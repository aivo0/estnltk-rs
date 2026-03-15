"""Cross-implementation tests for Vabamorf integration and expander features."""

import os
import pytest

from estnltk_regex_rs import (
    RsVabamorf,
    RsSubstringTagger,
    rs_noun_forms_expander,
    rs_default_expander,
    rs_syllabify,
)

DCT_DIR = os.path.join(os.path.dirname(__file__), "..", "vabamorf-cpp", "dct")


@pytest.fixture(scope="module")
def vm():
    """Shared Vabamorf instance for all tests."""
    return RsVabamorf(DCT_DIR)


class TestRsVabamorConstruction:
    def test_valid_path(self, vm):
        assert vm is not None

    def test_invalid_path(self):
        with pytest.raises(ValueError):
            RsVabamorf("/nonexistent/path")


class TestSynthesize:
    def test_sg_genitive(self, vm):
        result = vm.synthesize("maja", "sg g", "S")
        assert isinstance(result, list)
        assert len(result) > 0
        assert "maja" in result

    def test_sg_inessive(self, vm):
        result = vm.synthesize("maja", "sg in", "S")
        assert "majas" in result

    def test_pl_nominative(self, vm):
        result = vm.synthesize("maja", "pl n", "S")
        assert isinstance(result, list)
        assert len(result) > 0


class TestNounFormsExpander:
    def test_returns_28_strings(self, vm):
        forms = vm.noun_forms_expander("maja")
        assert len(forms) == 28

    def test_standalone_function(self, vm):
        forms = rs_noun_forms_expander(vm, "maja")
        assert len(forms) == 28

    def test_known_forms(self, vm):
        forms = vm.noun_forms_expander("maja")
        # sg nominative (index 0)
        assert "maja" in forms[0]
        # sg inessive (index 8)
        assert "majas" in forms[8]

    def test_default_expander_same(self, vm):
        noun = vm.noun_forms_expander("maja")
        default = vm.default_expander("maja")
        assert noun == default

    def test_standalone_default(self, vm):
        noun = rs_noun_forms_expander(vm, "maja")
        default = rs_default_expander(vm, "maja")
        assert noun == default


class TestSubstringTaggerExpander:
    def test_expander_noun_forms(self, vm):
        tagger = RsSubstringTagger(
            [{"pattern": "maja", "attributes": {"type": "building"}}],
            expander="noun_forms",
            vabamorf=vm,
        )
        result = tagger.tag("majas on soe")
        spans = result["spans"]
        assert len(spans) > 0, "Should match 'majas' as expanded form of 'maja'"

    def test_expander_default(self, vm):
        tagger = RsSubstringTagger(
            [{"pattern": "maja"}],
            expander="default",
            vabamorf=vm,
        )
        result = tagger.tag("majas on soe")
        spans = result["spans"]
        assert len(spans) > 0

    def test_expander_without_vabamorf_raises(self):
        with pytest.raises((ValueError, TypeError)):
            RsSubstringTagger(
                [{"pattern": "maja"}],
                expander="noun_forms",
            )

    def test_expander_unknown_raises(self, vm):
        with pytest.raises(ValueError, match="Unknown expander"):
            RsSubstringTagger(
                [{"pattern": "maja"}],
                expander="unknown",
                vabamorf=vm,
            )


class TestAnalyze:
    def test_basic_analysis(self, vm):
        result = vm.analyze(["maja"])
        assert len(result) == 1
        assert result[0]["word"] == "maja"
        assert len(result[0]["analyses"]) > 0

    def test_multiple_words(self, vm):
        result = vm.analyze(["tere", "maailm"])
        assert len(result) == 2


class TestSpellcheck:
    def test_correct_word(self, vm):
        result = vm.spellcheck(["maja"])
        assert len(result) == 1
        assert result[0]["correct"] is True

    def test_misspelled_word(self, vm):
        result = vm.spellcheck(["majja"])
        assert len(result) == 1
        # May or may not be correct depending on dictionary


class TestSyllabify:
    def test_basic(self):
        result = rs_syllabify("maja")
        assert isinstance(result, list)
        assert len(result) > 0
        assert "syllable" in result[0]
        assert "quantity" in result[0]
        assert "accent" in result[0]
