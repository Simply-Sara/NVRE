Scriptname IfN3 extends ObjectReference

Function F(string str)
	if str == "S0"
		Game.AddAchievement(20)
	endif
	if str == "S1"
		Game.AddAchievement(21)
	endif
	if str == "S2"
		Game.AddAchievement(22)
	endif
EndFunction
